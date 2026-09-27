use super::*;

fn make_symbol(id: u32, repair: bool) -> WireSymbol {
    WireSymbol {
        block_id: 0,
        payload_id: id,
        is_repair: repair,
        data: vec![0u8; 64],
        backend: FecBackend::RaptorQ,
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
// Correction deficit tests (paper §5.9)
// -----------------------------------------------------------------------

#[test]
fn test_deficit_tracks_sends_and_acks() {
    let mut deficit = CorrectionDeficit::new();
    assert_eq!(deficit.deficit(), 0.0);

    deficit.on_send(0, 1, 0.10);
    deficit.on_send(1, 1, 0.10);
    deficit.on_send(2, 2, 0.05);
    assert!((deficit.deficit() - 0.25).abs() < 1e-10);
    assert_eq!(deficit.pending_count(), 3);

    // ACK symbol 1
    assert!(deficit.on_ack(1));
    assert!((deficit.deficit() - 0.15).abs() < 1e-10);
    assert_eq!(deficit.pending_count(), 2);

    // ACK unknown symbol → no change
    assert!(!deficit.on_ack(99));
    assert!((deficit.deficit() - 0.15).abs() < 1e-10);
}

#[test]
fn test_deficit_cumulative_ack() {
    let mut deficit = CorrectionDeficit::new();
    for seq in 0..10 {
        deficit.on_send(seq, 1, 0.10);
    }
    assert!((deficit.deficit() - 1.0).abs() < 1e-10);

    deficit.on_ack_cumulative(4); // ACK 0..=4
    assert_eq!(deficit.pending_count(), 5);
    assert!((deficit.deficit() - 0.5).abs() < 1e-10);
}

#[test]
fn test_deficit_per_path() {
    let mut deficit = CorrectionDeficit::new();
    deficit.on_send(0, 1, 0.10);
    deficit.on_send(1, 2, 0.05);
    deficit.on_send(2, 1, 0.10);

    assert!((deficit.path_deficit(1) - 0.20).abs() < 1e-10);
    assert!((deficit.path_deficit(2) - 0.05).abs() < 1e-10);
    assert!((deficit.path_deficit(3) - 0.00).abs() < 1e-10);
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

#[test]
fn test_pacing_token_bucket_rate_and_burst() {
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new(clock.clone());
    sched.add_path(0);

    let path = sched.path_mut(0).unwrap();
    // SRTT = 100ms exactly (EWMA of identical samples).
    for _ in 0..4 {
        path.record_rtt_sample(millis(100));
    }
    path.cwnd = 200; // rate = 200/0.1 = 2000 symbols/sec
    path.pace_tokens = 0.0;

    clock.advance(millis(5)); // 2000/s × 5ms = 10 tokens
    sched.path_mut(0).unwrap().pace_refill();
    let tokens = sched.path(0).unwrap().pace_tokens();
    assert!((tokens - 10.0).abs() < 1e-6, "refill rate is cwnd/SRTT, got {tokens}");

    clock.advance(millis(100)); // would add 200 → capped at burst
    sched.path_mut(0).unwrap().pace_refill();
    let tokens = sched.path(0).unwrap().pace_tokens();
    // burst allowance = max(10, cwnd/8) = max(10, 25) = 25
    assert!((tokens - 25.0).abs() < 1e-6, "burst cap is max(10, cwnd/8), got {tokens}");

    // Batch-granular overdraft: consumption may push the bucket negative.
    let path = sched.path_mut(0).unwrap();
    path.consume_pace_tokens(30);
    assert!(path.pace_tokens() < 0.0);
    assert!(path.pace_delay() > Duration::ZERO);

    // Small-cwnd burst floor: max(10, cwnd/8) = 10.
    let path = sched.path_mut(0).unwrap();
    path.cwnd = 16;
    path.pace_tokens = 0.0;
    clock.advance(millis(1000));
    path.pace_refill();
    assert!((path.pace_tokens() - 10.0).abs() < 1e-6);
}

#[test]
fn test_copa_target_cwnd_units() {
    // Units doc-test: floor = SRTT = 100ms → dq clamps at 0.1ms.
    // rate = 1/(0.5 [1/sym] × 1e-4 [s]) = 20000 symbols/s
    // cwnd = 20000 [sym/s] × 0.1 [s] = 2000 symbols
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new(clock);
    sched.add_path(0);
    let path = sched.path_mut(0).unwrap();
    path.record_rtt_sample(millis(100));
    assert_eq!(path.copa_target_cwnd(), 2000);
}

#[test]
fn test_paced_ramp_reaches_block_scale_without_spurious_backoff() {
    // With symbol-paced sends the standing queue stays near zero, so at
    // C2-like parameters (10ms floor, no competing traffic) the ramp must
    // sail past one 64KB block (~56 symbols) within 15 SRTTs and never
    // back off. Batch-granular pacing fails exactly this: every block
    // burst self-queues above Bulk's 2.5ms threshold and cwnd pins just
    // under one block.
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new(clock.clone()); // Auto: 1.125 target
    sched.add_path(0);

    for round in 0..15 {
        // Token-paced send phase across one RTT: consume only what
        // the bucket allows, in 1ms steps (never a whole-block burst).
        for _ in 0..12 {
            let p = sched.path_mut(0).unwrap();
            p.pace_refill();
            let budget = p.pace_tokens().max(0.0) as u32;
            if budget > 0 {
                p.consume_pace_tokens(budget);
            }
            clock.advance(millis(1));
        }
        // Paced sends leave only sub-threshold jitter over the floor
        // (alternating 10.0/10.5ms; Auto's backoff needs > 11.25ms).
        let sample = if round % 2 == 0 { 10_000 } else { 10_500 };
        let p = sched.path_mut(0).unwrap();
        p.record_rtt_sample(Duration::from_micros(sample));

        let before = sched.path(0).unwrap().cwnd;
        sched.ack(0, before.min(64));
        let after = sched.path(0).unwrap().cwnd;
        assert!(
            after >= before,
            "paced sends must not trigger a backoff (round {round}): {before} -> {after}"
        );
    }
    let cwnd = sched.path(0).unwrap().cwnd;
    assert!(
        cwnd > 100,
        "ramp must clear one 64KB block (~56 symbols) at C2, got {cwnd}"
    );
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

    // The paced drain charges tokens only — in_flight must not move
    // between schedule and ack (as in net/mod.rs).
    sched.path_mut(0).unwrap().consume_pace_tokens(8);
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

        // Pacer: send from the carry under tokens; the batch timestamp
        // is stamped here (wire time), as in send_interleaved_batches.
        {
            let p = sched.path_mut(0).unwrap();
            p.pace_refill();
            let budget = (p.pace_tokens().max(0.0) as u32).min(carry);
            if budget > 0 {
                p.consume_pace_tokens(budget);
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

/// `RWM_HONEST_K` law + off-value property, at the engine feed site
/// (`record_rtt`), on a jittered series: RTprop (min over raw samples)
/// reads the distribution floor, the smoothed srtt reads near the mean
/// — so the smoothed K (windowed-min of the smoothed series over the
/// floor) reads high by ≈ mean/floor. The raw-fed tracker reads the
/// floor ratio ≈ 1. Gate off ⇒ `k_raw_ratio()` is None (nothing is fed,
/// nothing can consume it).
#[test]
fn k_raw_reads_the_jitter_floor_where_the_smoothed_min_reads_high() {
    let clock = Arc::new(MockClock::new());
    // Off: no ratio exists.
    let mut off = CopaState::new(clock.clone(), ProtocolHint::Bulk);
    off.record_rtt(Duration::from_millis(40));
    assert_eq!(off.k_raw_ratio(), None, "gate OFF ⇒ no raw K");

    // On: the raw-fed windowed min under ±25 ms uniform jitter around a
    // 40 ms base (the jit25 shape).
    let mut on = CopaState::new(clock.clone(), ProtocolHint::Bulk);
    on.force_k_raw();
    // The net-side smoothed tracker, fed the smoothed series at the 5 ms
    // refresh clock — exactly the engine's shipped K feed.
    let mut legacy_k = crate::net::EchoRatioMin::new(crate::net::PERCAP_K_HALF_WINDOW_US);
    let mut lcg: u64 = 42;
    let mut uniform = move || {
        lcg = lcg.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((lcg >> 33) as f64) / (u32::MAX as f64) // [0, 1)
    };
    let t0 = clock.now();
    for _ in 0..2000 {
        clock.advance(Duration::from_millis(5)); // ack + refresh cadence
        let jitter_ms = 50.0 * uniform() - 25.0; // ±25 ms
        let raw = Duration::from_secs_f64((0.040 + jitter_ms / 1e3).max(0.000_07));
        on.record_rtt(raw); // raw feed (the fix) + srtt/min_rtt as shipped
        // Shipped feed: smoothed srtt at the refresh clock.
        let now_us = clock.now().duration_since(t0).as_micros() as u64;
        legacy_k.observe_srtt_over_rtprop(on.srtt.unwrap(), on.min_rtt, now_us);
    }
    let k_raw = on.k_raw_ratio().expect("gate ON ⇒ raw K live");
    let k_legacy = legacy_k.k();
    // Direction + class, both ways: the smoothed
    // min sits near mean/floor ≈ 40/15 territory, far above 1; the raw
    // min reads the floor.
    assert!(
        k_legacy > 1.2,
        "the smoothed-series windowed min must read HIGH under wide jitter \
         (the jit25 inversion): k_legacy = {k_legacy}"
    );
    assert!(
        k_raw < 1.05,
        "the raw-fed windowed min must read the distribution floor: k_raw = {k_raw}"
    );
    assert!(
        k_legacy > k_raw * 1.2,
        "the bias must be the SMOOTHING's, removed by the raw feed: \
         legacy {k_legacy} vs raw {k_raw}"
    );
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

/// RTprop honesty under netem's clamped jitter (the `jit25` cell), at
/// component level, with the real estimator stack. Run explicitly:
///
/// ```text
///   cargo test --release -p raptorpath --lib -- --ignored --nocapture jit25_rtprop
/// ```
///
/// The cell's config (tools/l1/adv_cells.sh `jit25`): each
/// direction is `netem delay 20ms 25ms 25% rate 100mbit`, i.e. one-way
/// delay = clamp(20 ms + x·25 ms, 0) + ~108 µs serialization (1350 B at
/// 100 Mbit), x the 25%-autoregressively-correlated uniform variate on
/// [−1, 1] (netem get_crandom's linear form x_n = ρ·x_{n−1} + (1−ρ)·u_n,
/// identical in distribution class; approximation disclosed). An RTT
/// sample sums two independent directions. The AR(ρ=0.25) marginal
/// is narrower than uniform (sd ≈ 0.45 vs 0.58), so the clamp mass is
/// ~4%/direction, not the naive 10% — the model computes it rather than
/// assuming it.
///
/// Instrument: `CopaState::record_rtt` on a MockClock — the shipped
/// srtt EWMA (α = 1/8), the shipped 10 s min-window RTprop deque, and
/// the `RWM_HONEST_K` raw-fed `EchoRatioMin` (forced on) — at swept
/// RTT-sample cadences; plus direct windowed-min curves over the same
/// series for a window sweep.
///
/// Three curves (`[P3-JIT]` lines): (1) floor-sighting rate vs window
/// length, (2) windowed-min RTprop vs the distribution's true floor,
/// (3) the implied K elevation vs window.
///
/// Under the unloaded distribution the clamp floor is not rare at dense
/// cadences — every 10 s window re-sights the floor class, RTprop reads
/// ≪ the 40 ms base, K_raw → 1, and the law's window term would collapse.
/// In the rare-floor regime (sparse cadence) the fingerprint inverts:
/// RTprop keeps a once-seen deep min and the windowed K reads ≫ 1.5. An
/// in-cell K between those (≈ 1.0–1.5) therefore rides a floor the window
/// genuinely re-achieves — the loaded link's standing queue, not estimator
/// bias and not floor rarity.
#[test]
#[ignore = "measurement: run explicitly with --release --ignored --nocapture"]
fn jit25_rtprop_floor_sighting_under_netem_clamped_jitter() {
    const BASE_S: f64 = 0.020; // netem delay 20ms
    const JIT_S: f64 = 0.025; // ±25ms
    const RHO: f64 = 0.25; // 25% correlation
    const SER_S: f64 = 0.000_108; // 1350 B @ 100 Mbit, per direction
    const FLOOR_S: f64 = 2.0 * SER_S; // both jitter draws at the clamp
    const T_S: f64 = 30.0; // series length
    const RATE_SYM_S: f64 = 7_600.0; // the battery's measured arm-A class

    // One netem direction: AR-correlated uniform, clamped at 0.
    struct Dir {
        lcg: u64,
        x: f64,
    }
    impl Dir {
        fn new(seed: u64) -> Self {
            Self { lcg: seed, x: 0.0 }
        }
        fn next(&mut self) -> f64 {
            self.lcg = self
                .lcg
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let u = ((self.lcg >> 33) as f64) / (u32::MAX as f64) * 2.0 - 1.0;
            self.x = RHO * self.x + (1.0 - RHO) * u;
            (BASE_S + self.x * JIT_S).max(0.0) + SER_S
        }
    }

    println!(
        "[P3-JIT] model: clamp(20ms ± 25ms AR(0.25) uniform, 0) + {:.0} µs/dir; \
         true two-way floor = {:.2} ms; base RTT = {:.1} ms; cell rate class {} sym/s",
        SER_S * 1e6,
        FLOOR_S * 1e3,
        2.0 * BASE_S * 1e3,
        RATE_SYM_S
    );

    for &cad in &[20.0f64, 100.0, 1000.0, 7400.0] {
        let clock = Arc::new(MockClock::new());
        let mut cs = CopaState::new(clock.clone(), ProtocolHint::Bulk);
        cs.force_k_raw();
        let (mut fwd, mut back) = (Dir::new(42), Dir::new(7));
        let step = Duration::from_secs_f64(1.0 / cad);
        let n = (T_S * cad) as usize;
        let mut series: Vec<f64> = Vec::with_capacity(n);
        for _ in 0..n {
            clock.advance(step);
            let rtt = fwd.next() + back.next();
            series.push(rtt);
            cs.record_rtt(Duration::from_secs_f64(rtt));
        }
        let rtprop = cs.min_rtt().unwrap().as_secs_f64();
        let srtt = cs.srtt.unwrap().as_secs_f64();
        let k_raw = cs.k_raw_ratio().unwrap();
        let global_min = series.iter().copied().fold(f64::INFINITY, f64::min);
        // Window sweep: non-overlapping windows of W seconds — mean
        // windowed min, floor-sighting fraction, implied K = mean
        // windowed-min ÷ global min (the elevation of a W-horizon floor
        // over the long-run floor).
        print!(
            "[P3-JIT] cadence {cad:>6.0}/s: RTprop(10s win) {:.2} ms, srtt {:.1} ms, \
             srtt/RTprop {:.1}, K_raw {k_raw:.2}, global min {:.2} ms | implied \
             3T window term {:.0} sym (cell measured 396–714)\n",
            rtprop * 1e3,
            srtt * 1e3,
            srtt / rtprop,
            global_min * 1e3,
            RATE_SYM_S * k_raw.max(1.0) * rtprop,
        );
        let mut prev_mean = f64::INFINITY;
        for &w_s in &[0.5f64, 1.0, 2.0, 5.0, 10.0] {
            let wlen = ((w_s * cad) as usize).max(1);
            let mut mins = Vec::new();
            let mut sighted = 0usize;
            for chunk in series.chunks(wlen) {
                if chunk.len() < wlen {
                    break;
                }
                let m = chunk.iter().copied().fold(f64::INFINITY, f64::min);
                if m <= FLOOR_S + 0.001 {
                    sighted += 1;
                }
                mins.push(m);
            }
            let mean = mins.iter().sum::<f64>() / mins.len() as f64;
            println!(
                "[P3-JIT]   window {w_s:>4.1} s: mean windowed-min {:.2} ms, \
                 floor-sighting {:.0}% of windows, implied K(w) {:.2}",
                mean * 1e3,
                100.0 * sighted as f64 / mins.len() as f64,
                mean / global_min,
            );
            // A longer horizon can only read lower (min over a superset)
            // — "a derivable better floor" does not exist in the upward
            // direction the elevated limit would need.
            assert!(
                mean <= prev_mean * 1.02,
                "windowed min must be non-increasing in the horizon"
            );
            prev_mean = mean;
        }
        // The dense regime (any cadence ≥ ~1000/s): the floor is not
        // rare — RTprop reads the floor class, K_raw reads ≈ 1, and the
        // law's window term collapses to the sym class.
        if cad >= 1000.0 {
            assert!(
                rtprop < 0.005,
                "dense sampling must sight the clamp floor: RTprop {rtprop}"
            );
            assert!(
                k_raw < 1.5,
                "dense sampling re-achieves the floor in-window: K_raw {k_raw}"
            );
            assert!(
                srtt / rtprop > 8.0,
                "the unloaded distribution's srtt/RTprop is an order above \
                 the cell's measured 1.0–1.5 class: {}",
                srtt / rtprop
            );
        }
    }
    println!(
        "[P3-JIT] adjudication: the cell measured khr ≈ kraw ≈ 1.0–1.5 WITH an \
         elevated limit (window/rate ≈ 50–90 ms) — neither the dense-floor collapse \
         nor the rare-floor K ≫ 1.5 fingerprint. The in-cell series rides a \
         re-achieved floor far above the unloaded clamp floor: the standing queue \
         of the loaded 100 Mbit link. RTprop is honest w.r.t. its own window; the \
         elevation is real residence."
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

// ----- Pool-anchor store law (RWM_POOL_ANCHOR) ------------------------------

/// The burst-immunity + anchor-consumer-separation law: a steady send
/// process with the acks arriving in bursts must (a) drive the
/// ack-interval windowed-max (`record_delivery` via `on_ack` — the Copa
/// cwnd feed, deliberately unchanged) to a burst-peak over-read, while
/// (b) the pool-anchor send-interval rate — the N ≥ 2 store-cap law's
/// input — keeps reading ≈ the true send rate. One PathState, both
/// consumers, same clock.
#[test]
fn pool_anchor_send_rate_is_burst_immune_while_the_copa_feed_over_reads() {
    let clock = Arc::new(MockClock::new());
    let mut path = PathState::new(0, clock.clone());
    path.force_pool_anchor_feed(true);
    path.record_rtt_sample(millis(10)); // srtt/RTprop warm

    // Steady send process: 1 symbol/ms ≈ 1000 sym/s for 2 s, fed at the
    // real feed site (charge_in_flight = every wire send on this path).
    for _ in 0..2000 {
        path.charge_in_flight(1);
        path.release_in_flight(1);
        clock.advance(millis(1));
    }
    let sr = path
        .send_rate_anchor()
        .expect("send anchor warm within the window");
    assert!(
        (sr - 1000.0).abs() / 1000.0 < 0.25,
        "send-interval anchor reads ≈ truth (1000 sym/s), got {sr}"
    );

    // est-cadence ack clock: every ~100 ms a tight burst — a 1-ms-spaced
    // clump whose Δdelivered/Δt spikes ~200× the true rate. The ack-interval
    // windowed max latches the spike; the send anchor must not move. And
    // the send side bursts too: each ack burst frees store slots and the
    // admission-gated sender refills at emission speed — a ~5 ms bucket at
    // ~40k sym/s. A windowed max would latch that refill burst; the mean
    // the law reads must stay ≈ the true carried rate.
    for _ in 0..10 {
        // Steady send process between ack bursts.
        for _ in 0..93 {
            path.charge_in_flight(1);
            path.release_in_flight(1);
            clock.advance(millis(1));
        }
        path.on_ack(1); // re-arm last_delivered_time
        clock.advance(millis(2));
        path.on_ack(400); // ack burst peak: 400 / 2 ms = 200k sym/s
        // Store-refill send burst: 200 symbols in ~5 ms (~40k sym/s).
        for _ in 0..200 {
            path.charge_in_flight(1);
            path.release_in_flight(1);
            clock.advance(Duration::from_micros(25));
        }
    }
    let btlbw = path
        .btlbw_sym_per_s()
        .expect("legacy anchor established (the cwnd feed still runs)");
    assert!(
        btlbw > 10.0 * 1000.0,
        "the LEGACY ack-interval max must show the burst-peak over-read \
         (the unchanged Copa-feed channel), got {btlbw}"
    );
    // Carried truth over the burst phase: (93 + 200) sends per 100 ms
    // cycle ≈ 2 930 sym/s — the mean must read it; the 40k refill peaks
    // and the 200k ack peaks must both be invisible to it.
    let truth = (93.0 + 200.0) / 0.1;
    let sr_after = path.send_rate_anchor().expect("anchor still live");
    assert!(
        (sr_after - truth).abs() / truth < 0.35,
        "the pool anchor must be burst-immune: got {sr_after} vs carried truth {truth}"
    );
    // And the pool term derived from it stays in the truth class while the
    // ack-interval term reads the spike: the store-cap consumer reads the
    // send anchor, the cwnd consumer is left alone.
    let rtp = path.min_rtt().unwrap().as_secs_f64();
    let honest = crate::net::honest_store_cap(Some(sr_after * rtp), Some(sr_after), 1.0, 2.0)
        .unwrap();
    let legacy_bdp = path.copa_bdp_anchor().unwrap();
    assert!(
        legacy_bdp > 2.0 * (sr_after * rtp),
        "legacy BDP anchor carries the over-read: {legacy_bdp} vs honest pipe {}",
        sr_after * rtp
    );
    assert!(
        honest < 2.0 * (sr_after * rtp + sr_after * crate::net::HONEST_RECOVERY_ROUND_S),
        "honest cap term stays residence+runway-bounded, got {honest}"
    );
}

/// `RWM_POOL_ANCHOR=0` and N = 1 cost honesty: with the feed off,
/// `charge_in_flight` does no anchor work and the anchor reads None.
#[test]
fn pool_anchor_feed_off_is_inert() {
    let clock = Arc::new(MockClock::new());
    let mut path = PathState::new(0, clock.clone());
    path.force_pool_anchor_feed(false);
    path.record_rtt_sample(millis(10));
    for _ in 0..100 {
        path.charge_in_flight(1);
        clock.advance(millis(1));
    }
    assert_eq!(path.in_flight, 100, "in-flight accounting unchanged");
    assert!(
        path.send_rate_anchor().is_none(),
        "feed off ⇒ no send-anchor samples (byte-identical prior path)"
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

/// The derived loss count is `expected - received`, so `received` may
/// never exceed `expected` however the receiver's batch-gap estimate
/// moves. (The estimate is approximate by construction — see
/// `PathBatchTracker` — and an underflow here would feed the loss
/// estimator garbage.)
#[test]
fn ack_merge_counter_delta_never_lets_received_exceed_expected() {
    let clock = Arc::new(MockClock::new());
    let mut path = PathState::new(0, clock.clone());
    let (e, r) = path.ack_merge_counter_delta(10, 40);
    assert_eq!(e, 10);
    assert_eq!(r, 10, "received is clamped to expected, never above it");
    assert!(e >= r);
}

// ===================================================================
// The pinned placement cost table
//
// The placement arms (derived temperature `T = (sqrt6/pi) sigma_e/ref`,
// the HOL term, the derived `w_div`) are absent by default, and with the
// arms off the shipped law must be unchanged. That is provable from a
// table of hand-built states and the cost vectors the engine returns for
// them, to 1e-12, captured with every gate absent: an edit that moves any
// number in it has changed the shipped law. `place_costs` is `pub(crate)`
// for this pin; nothing else reads it.
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

/// The capture harness. Ignored by default -- run it with
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

/// The pin: the engine's `place_costs`, to 1e-12, over the
/// eight states above -- and the order, which the deterministic
/// tie-break makes reproducible (state 8 adds its paths 2, 0, 1 and
/// must still return 0, 1, 2).
#[test]
fn place_costs_match_the_pinned_table() {
    const PINNED: &[(&str, &[(PathId, f64)])] = &[
        ("single-idle", &[(0, 5.00000000000000000e-1)]),
        ("dual-symmetric-idle", &[(0, 5.00000000000000000e-1), (1, 5.00000000000000000e-1)]),
        ("dual-asymmetric-rtt", &[(0, 5.00000000000000000e-1), (1, 2.50000000000000000e0)]),
        ("dual-waterfill", &[(0, 9.00000000000000133e-1), (1, 9.00000000000000133e-1)]),
        ("dual-overdraft", &[(0, 3.00000000000000000e0), (1, 5.00000000000000000e-1)]),
        ("repair-fate", &[(0, 1.25000000000000000e0), (1, 7.50000000000000000e-1)]),
        ("cold-path", &[(0, 5.00000000000000000e-1), (1, 2.50000000000000000e0)]),
        ("triple-out-of-order", &[(0, 5.99999999999999978e-1), (1, 1.39999999999999991e0), (2, 2.39999999999999991e0)]),
    ];
    let states = cost_table_states();
    assert_eq!(states.len(), PINNED.len(), "the table lost a state");
    for ((name, sched, is_repair, covered), (pname, expect)) in
        states.into_iter().zip(PINNED.iter())
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
                 (delta {:.3e}) -- THE SHIPPED PLACEMENT LAW MOVED",
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
// The placement arms (paper §5.7, all three absent by default)
//
// The order of these tests is the order of the claims they defend:
// (1) with every arm absent the shipped law is byte-identical, in costs
// and in probabilities; (2) each arm's own limits are the ones the
// derivation states; (3) the shape checks -- knee continuity,
// monotonicity, dial continuity -- hold on the law itself; (4) the
// controls (N = 1, the tie-break) still hold with everything armed.
// ===================================================================

/// Gate-off byte-identity, the probability side. The pinned table
/// asserts the costs; this asserts that the softmax over them is
/// untouched too, to 1e-12 -- because the arms enter through the
/// temperature as well as through the cost, and a temperature change
/// leaves every cost fixed while moving every probability.
#[test]
fn every_arm_absent_leaves_the_probabilities_bit_identical() {
    for (name, mut sched, is_repair, covered) in cost_table_states() {
        sched.set_place_t_derived(false);
        sched.set_place_hol(false);
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
/// environment carries all three off, which is what makes the pinned
/// table an oracle for every other test in this file.
#[test]
fn the_placement_arms_are_absent_by_default() {
    let sched = Scheduler::new_with_hint(Arc::new(WallClock), ProtocolHint::Auto);
    assert!(!sched.place_t_derived(), "RWM_PLACE_T_DERIVED ships ABSENT");
    assert!(!sched.place_hol(), "RWM_PLACE_HOL ships ABSENT");
    assert!(!sched.place_wdiv_derived(), "RWM_PLACE_WDIV_DERIVED ships ABSENT");
}

/// Arm 1, the `sigma -> 0` limit. A candidate set whose ETA-error
/// dispersion is zero has `T = 0`, and the derived law must then be
/// exactly the argmin -- the shipped degenerate branch, reached with
/// `place_probs_with_temperature(.., 0.0)`. Same distribution, element by
/// element.
#[test]
fn the_derived_temperature_at_zero_dispersion_is_the_argmin() {
    let mut sched = cost_state(
        ProtocolHint::Auto,
        &[(0, Some(10), 10, 0), (1, Some(50), 10, 0)],
    );
    sched.set_place_t_derived(true);
    // A dispersion series that never moves: sigma = 0 at both paths. The
    // tau-lag admits a pair only inside `[tau, 2*tau]` of real time, so the
    // arrival instants are supplied rather than slept for.
    let t0 = std::time::Instant::now();
    for pid in [0u32, 1] {
        for k in 0..40u64 {
            let ts = 1_000 + k * 1_000 + u64::from(pid) * 1_000_000;
            sched.eta_mut().stamp(pid, ts, 5_000);
            sched.eta_mut().on_ack_at(
                pid,
                ts,
                10_000,
                10_000,
                t0 + Duration::from_micros(1_250 * k),
            );
        }
    }
    assert_eq!(sched.eta().sigma_us(0), Some(0), "the fixture must be zero-dispersion");
    let got = sched.place_probs(false, &[]);
    let want = sched.place_probs_with_temperature(false, &[], 0.0);
    assert_eq!(got, want, "sigma = 0 must resolve to the argmin exactly");
    // And the gauge says the cold rule did not fire -- a measured zero and
    // an absent measurement are different findings.
    sched.drain_place_bind();
    let l = sched.eta().line();
    assert!(l.contains("t_cold=0.0000"), "{l}");
    assert!(l.contains("t_eff=0.000000"), "{l}");
}

/// Arm 1's cold rule is stated and gauged, never a hidden constant.
/// With no path carrying a dispersion sample the temperature is the
/// shipped one and `t_cold` says so, so a reader can tell "the derived
/// law ran" from "the derived law had nothing to run on".
#[test]
fn the_derived_temperature_falls_back_to_the_shipped_one_and_counts_it() {
    let mut sched = cost_state(
        ProtocolHint::Auto,
        &[(0, Some(10), 10, 0), (1, Some(50), 10, 0)],
    );
    sched.set_place_t_derived(true);
    assert!((sched.place_temperature_eff() - PLACE_TEMPERATURE).abs() < 1e-15);
    sched.drain_place_bind();
    let l = sched.eta().line();
    assert!(l.contains("t_cold=1.0000"), "the cold rule must be READ, not assumed: {l}");
    assert!(l.contains("t_n=1"), "{l}");
}

/// Arm 1's own arithmetic, asserted absolutely and not ordinally: with
/// one measured dispersion of `sigma` microseconds and a reference SRTT of
/// `ref`, `T = (sqrt6/pi)*sigma/ref` to floating precision. The inverse is
/// the falsifiable claim: `T = 0.15 <=> sigma = 0.19238*ref`.
#[test]
fn the_derived_temperature_is_the_gumbel_variance_match() {
    let mut sched = cost_state(ProtocolHint::Auto, &[(0, Some(10), 10, 0)]);
    sched.set_place_t_derived(true);
    // A ramp, so a tau-lag pair spans a real difference: the error rises
    // 100 us per sample at 1.25 ms spacing, tau = 10 ms, exactly the
    // fixture `net::eta`'s own band test uses.
    let t0 = std::time::Instant::now();
    for k in 0..40u64 {
        let ts = 1_000 + k * 1_000;
        sched.eta_mut().stamp(0, ts, 5_000);
        sched.eta_mut().on_ack_at(
            0,
            ts,
            10_000 + 100 * k,
            10_000,
            t0 + Duration::from_micros(1_250 * k),
        );
    }
    let sig = sched.eta().sigma_us(0).expect("the fixture must measure a dispersion");
    assert!(sig > 0, "the fixture must measure a NONZERO dispersion");
    let ref_srtt = 0.010_f64;
    let want = place_gumbel_scale() * (sig as f64 / 1e6) / ref_srtt;
    assert!(
        (sched.place_temperature_eff() - want).abs() < 1e-12,
        "T must be (sqrt6/pi)*sigma/ref exactly"
    );
    // The inversion the paper states, checked as arithmetic: T = 0.15
    // asserts sigma/ref = 0.19238.
    assert!(
        ((PLACE_TEMPERATURE / place_gumbel_scale()) - 0.192_38).abs() < 1e-5,
        "the shipped T inverts to sigma_e = 0.19238*ref"
    );
}

/// Arm 2's own limit: a placement behind the frontier is free.
/// `now + E_i <= F_hat` gives `push = 0` and therefore `X_i = 0` exactly
/// -- not "small", zero -- which is the property that makes the term a
/// water-filling incentive rather than a slow-path penalty. Only the
/// wire price, three orders of magnitude down, remains.
#[test]
fn the_frontier_term_is_exactly_zero_behind_the_frontier() {
    assert_eq!(place_frontier_cost(0.5, 0.0, 0.0, 0.010, 0.0, 0.010), 0.0);
    // Any distance behind the frontier leaves the priced and the stall
    // legs both at zero; the wire price is all that is left.
    let only_wire = place_frontier_cost(50.0, 0.0, 0.030, 0.010, 1.12e-2, 0.010);
    assert!((only_wire - 1.12e-2 * 0.030 / 0.010).abs() < 1e-15);
    assert!(only_wire < 0.04, "the (a2) price is PREDICTED INERT against an O(1) load term");
}

/// Arm 2's knee is `C0` at `push = H`. The `(push - H)+` kink is a
/// corner, not a step: nudging `push` by +/-eps around `H` moves the cost
/// by O(eps), and the one-sided limits agree at the knee itself. A step
/// here would be exactly the behaviour-across-a-point defect the
/// no-mode-switch invariant forbids.
#[test]
fn the_frontier_knee_is_continuous_at_the_headroom() {
    let (delta, h, w, refs) = (0.5_f64, 0.010_f64, 0.0_f64, 0.010_f64);
    let at = place_frontier_cost(delta, h, 0.0, h, w, refs);
    for eps in [1e-6_f64, 1e-9, 1e-12] {
        let lo = place_frontier_cost(delta, h - eps, 0.0, h, w, refs);
        let hi = place_frontier_cost(delta, h + eps, 0.0, h, w, refs);
        // Both sides converge on the knee value, and the jump across it is
        // bounded by (delta + kappa)*2eps/ref -- a slope, not a step.
        let bound = (delta + PLACE_KAPPA) * 2.0 * eps / refs + 1e-15;
        assert!((hi - lo).abs() <= bound, "step at the knee: {lo} -> {hi} (eps {eps})");
        assert!((at - lo).abs() <= bound && (hi - at).abs() <= bound);
    }
}

/// Arm 2 is monotone in the push. A bigger frontier push costs more,
/// everywhere, on both sides of the knee. (And the slope beyond the knee
/// is the steeper one, which is what `kappa` is for.)
#[test]
fn the_frontier_term_is_monotone_in_the_push() {
    let (delta, h, refs) = (0.5_f64, 0.010_f64, 0.010_f64);
    let mut prev = f64::NEG_INFINITY;
    for i in 0..200 {
        let push = i as f64 * 0.0002;
        let c = place_frontier_cost(delta, push, 0.0, h, 0.0, refs);
        assert!(c >= prev - 1e-15, "cost fell as the push grew at push={push}");
        prev = c;
    }
    let below = place_frontier_cost(delta, h * 0.5, 0.0, h, 0.0, refs);
    let just = place_frontier_cost(delta, h, 0.0, h, 0.0, refs);
    let above = place_frontier_cost(delta, h * 1.5, 0.0, h, 0.0, refs);
    assert!(
        (above - just) > (just - below),
        "the stall leg must be the STEEPER one beyond H"
    );
}

/// The dial-continuity gate (CLAUDE.md). `delta` enters `X_i` as a
/// multiplier and nothing else, so the term is continuous and
/// non-decreasing in `delta` through each of the three named points --
/// checked at +/-2 % around every one of them, plus a dense sweep of the
/// whole dial. A behaviour step across a preset is a defect even if each
/// side is individually correct.
#[test]
fn the_frontier_term_is_continuous_through_every_named_point_on_the_dial() {
    let (push, h, refs) = (0.020_f64, 0.010_f64, 0.010_f64);
    for hint in [ProtocolHint::Realtime, ProtocolHint::Auto, ProtocolHint::Bulk] {
        let d = crate::scheduler::hint_delta_price(hint);
        let at = place_frontier_cost(d, push, 0.0, h, 0.0, refs);
        for f in [0.98_f64, 1.02] {
            let n = place_frontier_cost(d * f, push, 0.0, h, 0.0, refs);
            let jump = (n - at).abs();
            // The exact bound the form gives: |delta - delta'|*push/ref.
            let bound = (d * (f - 1.0)).abs() * push / refs + 1e-12;
            assert!(
                jump <= bound,
                "a 2 % nudge at {hint:?} (delta={d}) moved the term by {jump} > {bound}"
            );
        }
    }
    // Dense monotone sweep across the whole dial, presets included.
    let mut prev = f64::NEG_INFINITY;
    for i in 0..=400 {
        let d = 0.001_f64 * 10f64.powf(i as f64 / 80.0);
        let c = place_frontier_cost(d, push, 0.0, h, 0.0, refs);
        assert!(c >= prev - 1e-12, "the dial sweep is not monotone at delta={d}");
        prev = c;
    }
}

/// Arm 2, end to end: the term reaches `place_costs` and its gauges
/// fire. A pure-function test proves the form; this proves the wiring
/// (docs/measurement-discipline.md rule 1). With `F_hat` set far in the past
/// every candidate pushes the frontier hard, so the `s_i > H` bind is
/// taken and the costs are strictly above the shipped ones.
#[test]
fn the_frontier_term_reaches_place_costs_and_gauges_its_bind() {
    let mut sched = cost_state(
        ProtocolHint::Auto,
        &[(0, Some(10), 10, 0), (1, Some(50), 10, 0)],
    );
    let shipped = sched.place_costs(false, &[]);
    // A stamp in the distant past: `now` is far beyond it, so every
    // candidate lands ahead of the frontier by more than any headroom.
    sched.eta_mut().stamp(0, 1_000, 1);
    sched.set_place_hol(true);
    let armed = sched.place_costs(false, &[]);
    for ((_, a), (_, b)) in armed.iter().zip(shipped.iter()) {
        assert!(a > b, "the frontier term must CHARGE a frontier push: {a} vs {b}");
    }
    sched.drain_place_bind();
    let l = sched.eta().line();
    assert!(l.contains("hol_sh=1.0000"), "the s_i > H bind must be gauged: {l}");
    assert!(l.contains("hol_n=2"), "{l}");
    assert!(!l.contains("hol_w=-"), "W must print at its live value: {l}");
}

/// The execution witness itself. The `hol_mv` counter is what
/// distinguishes "the term was computed" from "the term decided". It is
/// zero for a symmetric pair (nothing to move) and nonzero when the
/// frontier term genuinely reverses the argmin -- built here by giving the
/// shipped-cheaper path a backlog large enough that its arrival, and only
/// its arrival, clears the frontier.
#[test]
fn the_argmin_witness_counts_only_real_reversals() {
    // Symmetric: no reversal is possible, and the witness says 0.
    let mut sym = cost_state(
        ProtocolHint::Auto,
        &[(0, Some(10), 10, 0), (1, Some(10), 10, 0)],
    );
    sym.eta_mut().stamp(0, 1_000, 1);
    sym.set_place_hol(true);
    let _ = sym.place_costs(false, &[]);
    sym.drain_place_bind();
    let (_, _, moved, calls, _) = sym.eta().hol_gauges();
    assert_eq!((moved, calls), (0, 1), "a symmetric pair has no argmin to move");

    // Asymmetric, with `F_hat` between the two arrivals: path 0 is the
    // shipped argmin and lands ahead of the frontier; path 1 is dearer by
    // the shipped law but lands behind it. The stall leg is worth more
    // than the load gap, so the term reverses the pick.
    let mut sched = cost_state(
        ProtocolHint::Realtime,
        &[(0, Some(10), 10, 9), (1, Some(11), 10, 0)],
    );
    let base = sched.place_costs(false, &[]);
    let base_arg = if base[0].1 <= base[1].1 { 0 } else { 1 };
    // `F_hat` a hair ahead of the cheaper path's arrival: path 0's own
    // backlog pushes it past the frontier, path 1's idle propagation does
    // not.
    let now_us = place_wall_now_us();
    sched.eta_mut().stamp(0, now_us, 6_000);
    sched.set_place_hol(true);
    let armed = sched.place_costs(false, &[]);
    let armed_arg = if armed[0].1 <= armed[1].1 { 0 } else { 1 };
    sched.drain_place_bind();
    let (_, _, moved, calls, _) = sched.eta().hol_gauges();
    assert_eq!(calls, 1);
    assert_eq!(
        moved,
        u64::from(base_arg != armed_arg),
        "the witness must count exactly the reversals that happened \
         (base {base_arg} -> armed {armed_arg}; {base:?} -> {armed:?})"
    );
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

/// The `N = 1` control, with every arm on. A softmax over a singleton
/// is 1 at every temperature and every cost, so the law is the identity on
/// a single path -- which is why single-path cells are controls for every
/// placement arm.
#[test]
fn a_single_path_is_the_identity_with_every_arm_armed() {
    let mut sched = cost_state(ProtocolHint::Auto, &[(0, Some(10), 10, 3)]);
    sched.set_place_t_derived(true);
    sched.set_place_hol(true);
    sched.set_place_wdiv_derived(true);
    sched.eta_mut().stamp(0, 1_000, 1);
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
    sched.set_place_t_derived(true);
    sched.set_place_hol(true);
    sched.set_place_wdiv_derived(true);
    sched.eta_mut().stamp(3, 1_000, 1);
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
