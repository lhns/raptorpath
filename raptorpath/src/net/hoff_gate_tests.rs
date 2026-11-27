//! Red-first tests of the hand-off gate (paper §6.8; registration
//! `reg-handoff-gate.md` §2 item 3), with the absolute anchors of
//! `sender-queue-law.md` §7.
//!
//! The gate's decisions are pure functions of their inputs, so the tests drive
//! the real gate (`HandoffGate`, `Scheduler::place_symbol_among`) with
//! synthetic `PathIn` readings, and a small virtual-time model stands in for
//! quinn's pacer and the owner. Wiring at the engine seams (`net/mod.rs`,
//! `emit_source.rs`, the owner) is pinned by source, the repo's form for a
//! call-site invariant.

use super::hoff::{HoffGauge, PathIn};
use super::hoff_gate::{
    admits, classify_first_send, owner_republishes, poll_armed, quinn_discards_found, r_eff, Bypass, FsClass,
    HandoffGate, Terms,
};
use crate::transport::io_owner::{own_gap_us, SendKind};
use crate::scheduler::{Scheduler, WallClock};
use crate::transport::io_owner::WorkHist;
use std::sync::Arc;

const S: u64 = 1233;

/// A reading with backlog `b`, datagram size S, window `cwnd` and RTT `srtt_us`.
fn pin(b: u64, cwnd: u64, srtt_us: u64) -> PathIn {
    PathIn { staged: 1000 + b, staged_bytes: (1000 + b) * S, tx: 1000, cwnd, qwin: cwnd, srtt_us, ..Default::default() }
}

const LEG1: Terms = Terms { tau_o: Some(0.002), l_o: 16e-6, tau_p: Some(0.00206), l_p: 96e-6 };
const LEG2: Terms = Terms { tau_o: Some(0.002), l_o: 10e-6, tau_p: Some(0.00207), l_p: 557e-6 };
const COLD: Terms = Terms { tau_o: None, l_o: 16e-6, tau_p: None, l_p: 96e-6 };

fn near(v: Option<f64>, want: f64) -> bool {
    v.is_some_and(|v| (v - want).abs() < 0.005)
}

fn flat(s: &str) -> String {
    s.split_whitespace().collect()
}

// ── 1. the gate decision, absolute ──────────────────────────────────────────

/// c2 leg 1: cwnd 280 000 B, srtt 22 ms, L = 4.172 ms → β = 53.83: hand at 53,
/// not at 54. Second leg: β = 12.29, hand at 12, not at 13. Cold start (both
/// timers at the 1 ms floor): L = 2.112 ms, β = 27.25.
#[test]
fn gate_decision_absolute_anchors() {
    let mut g = HandoffGate::new(true);
    g.fix_terms(LEG1);
    assert!(near(g.beta(0, &pin(0, 280_000, 22_000)), 53.83), "{:?}", g.beta(0, &pin(0, 280_000, 22_000)));
    assert!(g.admit(0, &pin(53, 280_000, 22_000)), "hands at B = 53");
    assert!(!g.admit(0, &pin(54, 280_000, 22_000)), "does not hand at B = 54");

    g.fix_terms(LEG2);
    assert!(near(g.beta(1, &pin(0, 230_000, 88_000)), 12.29), "{:?}", g.beta(1, &pin(0, 230_000, 88_000)));
    assert!(g.admit(1, &pin(12, 230_000, 88_000)), "hands at B = 12");
    assert!(!g.admit(1, &pin(13, 230_000, 88_000)), "does not hand at B = 13");

    g.fix_terms(COLD);
    assert!(near(g.beta(0, &pin(0, 280_000, 22_000)), 27.25), "{:?}", g.beta(0, &pin(0, 280_000, 22_000)));
    // The pure form agrees: B < β, never closed at B = 0 or on an undefined β.
    assert!(admits(53, Some(53.83)) && !admits(54, Some(53.83)));
    assert!(admits(0, Some(0.0)) && admits(10_000, None));
}

/// Cold start liveness: no window, no RTT or no datagram size yet is an
/// undefined β, and an undefined β admits.
#[test]
fn cold_start_admits() {
    let mut g = HandoffGate::new(true);
    g.fix_terms(COLD);
    assert!(g.beta(0, &PathIn::default()).is_none());
    assert!(g.admit(0, &pin(500, 280_000, 0)), "no RTT yet");
    assert!(g.admit(0, &pin(500, 0, 22_000)), "no window yet");
    assert!(g.admit(0, &PathIn { staged: 0, ..pin(0, 280_000, 22_000) }), "no datagram size yet");
}

/// B_i nets out what the owner dropped before quinn (shim loss, send errors),
/// or a lost datagram would ratchet `staged − tx` upward and wedge the gate.
#[test]
fn backlog_nets_out_owner_drops() {
    let i = PathIn { staged: 100, tx: 40, dropped: 10, ..Default::default() };
    assert_eq!(i.backlog(), 50);
}

/// quinn discards queued datagrams without a tx (drop-oldest on a full send
/// buffer, oversize after an MTU shrink). Each is a permanent +1 on B_i unless
/// the owner finds it: when quinn's queue reads empty (`space == capacity`,
/// read before `tx`), `handed − tx − counted` is exactly the discarded.
/// Model: a queue of `CAP` datagrams, drop-oldest; on slow leg 2 (β = 12.29).
#[test]
fn quinn_discards_return_to_zero_and_the_gate_reopens() {
    const CAP: u64 = 10;
    let cap_bytes = (CAP * S) as usize;
    let mut g = HandoffGate::new(true);
    g.fix_terms(LEG2);
    let (mut staged, mut handed, mut tx, mut counted) = (0u64, 0u64, 0u64, 0u64);
    let mut queued = 0u64;
    let b = |staged: u64, tx: u64, counted: u64| PathIn { staged, tx, quinn_discards: counted, ..pin(0, 230_000, 88_000) }.backlog();
    for round in 0..6 {
        // A burst of 30 into a 10-deep queue: 20 evicted silently.
        for _ in 0..30 {
            staged += 1;
            handed += 1;
            queued = (queued + 1).min(CAP);
        }
        // Queue non-empty: nothing is claimed discarded.
        let space = cap_bytes - (queued * S) as usize;
        assert_eq!(quinn_discards_found(space, cap_bytes, handed, tx, counted), 0, "round {round}: queue holds data");
        // quinn transmits what is left; the queue empties.
        tx += queued;
        queued = 0;
        let found = quinn_discards_found(cap_bytes, cap_bytes, handed, tx, counted);
        assert_eq!(found, 20, "round {round}");
        counted += found;
        assert_eq!(quinn_discards_found(cap_bytes, cap_bytes, handed, tx, counted), 0, "found once");
        assert_eq!(b(staged, tx, counted), 0, "round {round}: B back to 0");
        let i = PathIn { staged, staged_bytes: staged * S, tx, quinn_discards: counted, ..pin(0, 230_000, 88_000) };
        assert!(g.admit(1, &i), "round {round}: the gate reopens");
    }
    // Without the accounting the leak closes the leg for good (B ≥ β = 12.29).
    assert!(!g.admit(1, &PathIn { staged, staged_bytes: staged * S, tx, ..pin(0, 230_000, 88_000) }));
}

/// The owner reads space, then tx, in one publish; no hand sits between.
#[test]
fn quinn_discards_are_read_space_before_tx() {
    let o = include_str!("../transport/io_owner.rs");
    let p = o.split("fn publish(").nth(1).expect("publish");
    let (sp, st, f) = (
        p.find("datagram_send_buffer_space()").expect("space"),
        p.find("conn.stats()").expect("stats"),
        p.find("quinn_discards_found(").expect("found"),
    );
    assert!(sp < st && st < f, "space, then tx, then the residue");
    assert!(include_str!("../transport/quic.rs").matches("datagram_send_buffer_size(DATAGRAM_SEND_BUFFER)").count() == 2);
}

// ── 2. no dial is read; the wiring is where the law says ───────────────────

#[test]
fn no_dial_is_read_and_the_seams_are_wired() {
    // The gate's own source names no hint, δ, ρ or reliability bit.
    let src: String = include_str!("hoff_gate.rs")
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
        .to_lowercase();
    for tok in ["hint", "delta", "rho", "reliable", "protocol", "bulk", "realtime", "window_rho", "δ", "ρ"] {
        assert!(!src.contains(tok), "hoff_gate.rs reads `{tok}`");
    }
    // Wired at the seams, behind the arm only.
    let m = flat(include_str!("mod.rs"));
    let e = flat(include_str!("emit_source.rs"));
    let o = flat(include_str!("../transport/io_owner.rs"));
    let q = flat(include_str!("../transport/quic.rs"));
    assert!(m.contains("letgate_paused=if"), "the loop top computes the gate's pause");
    assert!(m.contains("poll_armed(tx_paused,gate_paused)"), "the 1 ms poll is armed on the gate's own pause");
    assert!(m.contains("tun.read_packet(),if!tx_paused&&!gate_paused"), "the intake read honours the pause");
    assert!(m.contains("lettx_paused=reliable&&(store_len>=effective_store_cap||cwnd_full);"), "tx_paused is untouched");
    assert!(e.contains("ctx.hg.on()") && e.contains("place_symbol_among(false") && e.contains("place_symbol_among(true"));
    assert!(e.contains("ctx.hg.spare_all()"), "the clamp reads the predicate at the repair seam");
    assert!(o.contains("owner_republishes(") && o.contains("republish=Some((crate::clock::now()"), "the owner republishes while it holds unsent data");
    assert!(q.contains("g.hoff_gate"), "the arm arms the RouteProbe");
}

// ── 3. the clamp: last-turn predicate ───────────────────────────────────────

/// One 36/s call: `n` sources, one turn each. `closed(k)` says whether turn k
/// finds the path closed with a first send waiting (the source then waits for
/// the next, open, turn). Returns repairs sent, each in its source's turn.
fn call(g: &mut HandoffGate, n: usize, r_law: f64, closed_before: impl Fn(usize) -> bool) -> u64 {
    let (open, shut) = ([(0u32, pin(0, 280_000, 22_000))], [(0u32, pin(500, 280_000, 22_000))]);
    let (mut debt, mut repairs, mut t) = (0.0, 0u64, 0u64);
    for k in 0..n {
        if closed_before(k) {
            t += 1000;
            g.begin_turn(t, true, false, &shut); // refused: waiting, B ≥ β
        }
        t += 1000;
        assert!(!g.begin_turn(t, true, false, &open), "the source is handed in an open turn");
        assert!(g.check(true, &open));
        g.note_first_send(0);
        debt += r_eff(r_law, g.spare_all());
        while debt >= 1.0 {
            debt -= 1.0;
            repairs += 1;
        }
    }
    repairs
}

#[test]
fn predicate_dry_call_sends_exactly_r_and_overload_sends_none() {
    let mut g = HandoffGate::new(true);
    g.fix_terms(LEG1);
    assert_eq!(g.spare_all(), f64::INFINITY, "cold start reads +inf");
    assert_eq!(call(&mut g, 1000, 0.25, |_| false), 250, "each turn ends dry: r_law");
    let mut g = HandoffGate::new(true);
    g.fix_terms(LEG1);
    assert_eq!(call(&mut g, 1000, 0.25, |_| true), 0, "the path refuses first sends: no repair");
}

#[test]
fn predicate_latches_through_store_full_turns_and_resets_when_a_turn_ends_dry() {
    let (open, shut) = ([(0u32, pin(0, 280_000, 22_000))], [(0u32, pin(500, 280_000, 22_000))]);
    let mut g = HandoffGate::new(true);
    g.fix_terms(LEG1);
    // A turn that admits a source and then closes the gate refuses the next one.
    g.begin_turn(1000, true, false, &open); // A
    assert!(g.check(true, &open));
    assert!(!g.check(true, &shut), "the gate closed behind the first source");
    // B ends because the store is full: it neither refused nor updates anything.
    g.begin_turn(2000, true, true, &open);
    assert_eq!(g.spare_all(), 0.0, "spare = 0 after a turn that admitted and then closed");
    g.begin_turn(3000, true, false, &open); // C
    assert_eq!(g.spare_all(), 0.0, "a store-full turn leaves the predicate unchanged");
    // C found room for everything waiting; D (dry) reads +inf.
    g.begin_turn(4000, false, false, &open); // D
    assert_eq!(g.spare_all(), f64::INFINITY, "a turn that ended dry reads +inf");
}

/// N = 2, leg 0 saturated (every turn closed) and leg 1 dry: 250 repairs, all
/// placed on leg 1, and the scheduler-wide spare reads +inf. Both legs
/// closed: none.
#[test]
fn predicate_two_legs_repair_goes_to_the_dry_leg() {
    let mut sched = Scheduler::new(Arc::new(WallClock));
    sched.add_path(0);
    sched.add_path(1);
    let ins = [(0u32, pin(500, 280_000, 22_000)), (1u32, pin(0, 280_000, 22_000))];
    let mut g = HandoffGate::new(true);
    g.fix_terms(LEG1);
    let (mut debt, mut on_leg1, mut repairs) = (0.0, 0u64, 0u64);
    for k in 0..1000u64 {
        g.begin_turn(1000 * (k + 1), true, false, &ins);
        assert_eq!(g.spare_all(), f64::INFINITY, "some live path has room");
        debt += r_eff(0.25, g.spare_all());
        while debt >= 1.0 {
            debt -= 1.0;
            repairs += 1;
            let p = sched.place_symbol_among(true, &[], &|id| g.spare_of(id) > 0.0);
            on_leg1 += (p == Some(1)) as u64;
        }
    }
    assert_eq!(repairs, 250, "r_law over 1 000 sources");
    assert_eq!(on_leg1, repairs, "every repair is placed on the dry leg");
    let shut = [(0u32, pin(500, 280_000, 22_000)), (1u32, pin(500, 280_000, 22_000))];
    g.begin_turn(2_000_000, true, false, &shut);
    g.begin_turn(2_001_000, true, false, &shut);
    assert_eq!(g.spare_all(), 0.0, "both legs closed: no repair");
}

// ── 4/5. the two-leg burst and the resend's wait (virtual-time model) ───────

struct Out {
    b1_over_beta: f64,
    b1_max: u64,
    b1_over_400_s: f64,
    starve: f64,
    resend_wait_max_ms: f64,
    beta1: f64,
}

/// Two legs at 13 100 and 2 640 datagrams/s behind a pooled store of 4 048
/// (the +2 000 step on c2's 2 048). `None` = no gate: a producer that takes
/// 64 per turn, split 5:1, every 2.06 ms. `Some(g)` = the gate: per-datagram
/// admission against B_i = staged − the owner's last published tx, the owner
/// republishing every 2.0 ms while it holds unsent data. Quinn drains each leg
/// at its rate. Exempt resends are handed to leg 1 every 10 ms.
fn two_leg_burst(mut gate: Option<&mut HandoffGate>) -> Out {
    const DT: u64 = 100; // µs
    let x = [13_100.0f64, 2_640.0];
    let srtt = [21_000u64, 88_000];
    let cwnd: Vec<u64> = (0..2).map(|i| (x[i] * S as f64 * srtt[i] as f64 / 1e6 / 1.25) as u64).collect();
    let (mut staged, mut tx_true, mut view_tx) = ([1000u64; 2], [1000.0f64; 2], [1000u64; 2]);
    let (mut next_wake, mut next_pub, mut next_resend) = (0u64, 0u64, 0u64);
    let (mut rr, mut samples, mut over_beta, mut starve, mut b1_max, mut over400) = (0u32, 0u64, 0u64, 0u64, 0u64, 0u64);
    let mut resend_wait_max = 0.0f64;
    let mut beta1 = 0.0;
    let mut refg = HandoffGate::new(true);
    refg.fix_terms(LEG2);
    let reading = |i: usize, staged: &[u64; 2], view_tx: &[u64; 2]| PathIn {
        staged: staged[i],
        staged_bytes: staged[i] * S,
        tx: view_tx[i],
        cwnd: cwnd[i],
        qwin: cwnd[i],
        srtt_us: srtt[i],
        ..Default::default()
    };
    let total_us = 6_000_000u64;
    let mut t = 0u64;
    while t < total_us {
        // quinn: each leg's pacer releases at its rate while its queue is non-empty
        for i in 0..2 {
            tx_true[i] = (tx_true[i] + x[i] * DT as f64 / 1e6).min(staged[i] as f64);
        }
        // owner: republish while it holds unsent data (the gate arm); every 2.0 ms
        if t >= next_pub {
            next_pub = t + 2000;
            for i in 0..2 {
                if owner_republishes(true, staged[i], 0, view_tx[i]) || gate.is_none() {
                    view_tx[i] = tx_true[i] as u64;
                }
            }
        }
        // exempt resend to leg 1: waits behind what quinn holds, counts in B
        if t >= next_resend {
            next_resend = t + 10_000;
            if t > 1_000_000 {
                resend_wait_max = resend_wait_max.max((staged[1] as f64 - tx_true[1]) / x[1] * 1e3);
            }
            staged[1] += 1;
        }
        // producer
        if t >= next_wake {
            let store_full = |s: &[u64; 2], tx: &[f64; 2]| (s[0] as f64 - tx[0]) + (s[1] as f64 - tx[1]) >= 4048.0;
            let mut closed_after = false;
            let mut emitted = 0;
            loop {
                let ins = [(0u32, reading(0, &staged, &view_tx)), (1u32, reading(1, &staged, &view_tx))];
                if store_full(&staged, &tx_true) {
                    closed_after = true;
                    break;
                }
                let path = match gate.as_deref_mut() {
                    Some(g) => {
                        if emitted == 0 {
                            g.begin_turn(t, true, false, &ins);
                        }
                        if !g.check(true, &ins) {
                            closed_after = true;
                            break;
                        }
                        let open = g.open_ids(&ins);
                        let b = |i: u32| (staged[i as usize] - view_tx[i as usize]) as f64 / x[i as usize];
                        *open.iter().min_by(|a, c| b(**a).total_cmp(&b(**c))).expect("an open path") as usize
                    }
                    None => {
                        rr = (rr + 1) % 6;
                        (rr == 0) as usize
                    }
                };
                staged[path] += 1;
                emitted += 1;
                if emitted >= 64 && gate.is_none() {
                    closed_after = true;
                    break;
                }
                if emitted >= 100_000 {
                    break;
                }
            }
            next_wake = if closed_after { t + 2060 } else { t };
        }
        // samples from 1 s on
        if t >= 1_000_000 && t % 1000 == 0 {
            samples += 1;
            let b1 = (staged[1] as f64 - tx_true[1]).max(0.0);
            b1_max = b1_max.max(b1 as u64);
            if gate.is_some() {
                // The bound is the law's (L = LEG2), whatever the gate under test reads.
                beta1 = refg.beta(1, &reading(1, &staged, &view_tx)).unwrap_or(f64::INFINITY);
                over_beta += (b1 > beta1 + 64.0) as u64;
            }
            starve += (b1 < 1.0) as u64;
            over400 += (b1 > 400.0) as u64;
        }
        t += DT;
    }
    Out {
        b1_over_beta: over_beta as f64 / samples as f64,
        b1_max,
        b1_over_400_s: over400 as f64 / 1000.0,
        starve: starve as f64 / samples as f64,
        resend_wait_max_ms: resend_wait_max,
        beta1,
    }
}

/// The reproduction (§1.2), green on any tree: without the gate quinn's queue
/// on the slow leg stays above 400 datagrams for seconds, and a resend handed
/// there waits about half a second.
#[test]
fn without_the_gate_the_slow_leg_queue_builds_and_resends_wait() {
    let o = two_leg_burst(None);
    assert!(o.b1_over_400_s >= 2.0, "B1 > 400 for {} s", o.b1_over_400_s);
    assert!(o.resend_wait_max_ms > 150.0, "resend wait {} ms", o.resend_wait_max_ms);
}

/// RED without the gate: with it, B1 ≤ β1 + one batch (64) on ≥ 99 % of
/// samples, and the slow leg starves ≤ 1 %.
#[test]
fn two_leg_burst_is_bounded_by_the_gate() {
    let mut g = HandoffGate::new(true);
    g.fix_terms(LEG2);
    let o = two_leg_burst(Some(&mut g));
    assert!(o.beta1 > 11.0 && o.beta1 < 14.0, "β1 = {}", o.beta1);
    assert!(o.b1_over_beta <= 0.01, "B1 > β1 + 64 on {:.1} % of samples (max {})", o.b1_over_beta * 100.0, o.b1_max);
    assert!(o.b1_max as f64 <= o.beta1 + 64.0);
    assert!(o.starve <= 0.01, "starve {:.3}", o.starve);
}

/// Mutation: L × 100 (β × 100) must turn the bound red.
#[test]
fn two_leg_burst_bound_is_sensitive_to_beta() {
    let mut g = HandoffGate::new(true);
    g.fix_terms(Terms { tau_o: Some(0.2), l_o: 16e-6, tau_p: Some(0.206), l_p: 96e-6 });
    let o = two_leg_burst(Some(&mut g));
    assert!(o.b1_over_beta > 0.01, "a β 100x too large must break the bound (max B1 {})", o.b1_max);
}

/// RED without the gate (≈ 0.5 s): a resend handed to leg 1 waits ≤ β1/x1 + g.
#[test]
fn resend_waits_at_most_beta_over_rate_plus_one_loop() {
    let mut g = HandoffGate::new(true);
    g.fix_terms(LEG2);
    let o = two_leg_burst(Some(&mut g));
    assert!(o.beta1 > 11.0 && o.beta1 < 14.0, "β1 = {}", o.beta1);
    let bound_ms = o.beta1 / 2_640.0 * 1e3 + 2.06 + 2.0; // β/x + the producer poll + the owner's republish lag
    assert!(o.resend_wait_max_ms <= bound_ms, "resend waited {} ms > {} ms", o.resend_wait_max_ms, bound_ms);
}

// ── 6. β across a silence ───────────────────────────────────────────────────

/// At c2's inputs a 1.5 s silence (no hand-off, no ack, no inbound, no new
/// sample) is followed by β within ±10 % of its steady 53.83. Red on the
/// measured-interval form, where β ≈ 19 000.
#[test]
fn beta_is_bounded_across_a_silence() {
    let mut g = HandoffGate::new(true);
    let (work, timer) = (WorkHist::default(), WorkHist::default());
    for _ in 0..200 {
        work.record(16_000); // ℓ̂_o ≈ 16 µs
        timer.record(2_000_000); // τ̂_o ≈ 2.0 ms
    }
    for k in 0..200u64 {
        g.paused_wait(k * 4_000, 2_060); // τ̂_p
        g.iter_work(k * 4_000, 96); // ℓ̂_p
    }
    g.owner_hists(0, 800_000, &work.snapshot(), &timer.snapshot());
    let i = pin(0, 280_000, 22_000);
    g.begin_turn(800_000, false, false, &[(0, i)]);
    let steady = g.beta(0, &i);
    assert!(steady.is_some_and(|b| (48.4..=59.2).contains(&b)), "steady β = {steady:?}");
    // 1.5 s of silence: no new sample, no hand-off.
    g.begin_turn(2_300_000, false, false, &[(0, i)]);
    let after = g.beta(0, &i);
    assert!(after.is_some_and(|b| (48.4..=59.2).contains(&b)), "β after the silence = {after:?}");
}

// ── 7. reopen with no inbound traffic (virtual time, a logic test) ──────────

/// A view published 0.5 ms before the owner's last I/O closed the gate (B seen
/// 100 ≥ β 53.8). The owner then idles, no inbound datagram, while quinn drains
/// at 0.2 ms. Returns the virtual µs from the owner's last I/O to the producer's
/// next hand-off, or `None` within 100 ms. `armed` = the owner's republish rule;
/// `lat_o`/`lat_p` the timer periods.
fn reopen_us(armed: bool, poll_phase: i64, lat_o: i64, lat_p: i64) -> Option<i64> {
    let mut g = HandoffGate::new(true);
    g.fix_terms(LEG1);
    let (staged, handed) = (1100u64, 1100u64);
    let (mut view_tx, mut last_full, mut next_poll) = (1000u64, -500i64, poll_phase);
    let mut polling = true;
    let mut t = 0i64;
    while t < 100_000 {
        let true_tx = if t >= 200 { 1100 } else { 1000 };
        if t >= last_full + lat_o && owner_republishes(armed, handed, 0, view_tx) {
            view_tx = true_tx;
            last_full = t;
        }
        if polling && t >= next_poll {
            let i = PathIn { staged, staged_bytes: staged * S, tx: view_tx, cwnd: 280_000, qwin: 280_000, srtt_us: 22_000, ..Default::default() };
            let paused = g.begin_turn(t as u64, true, false, &[(0, i)]);
            if !paused {
                return Some(t);
            }
            polling = poll_armed(false, paused);
            next_poll = t + lat_p;
        }
        t += 10;
    }
    None
}

#[test]
fn reopen_without_inbound_traffic_within_2_2_ms() {
    for phase in [0, 250, 500, 750, 999] {
        let t = reopen_us(true, phase, 1000, 1000);
        assert!(t.is_some_and(|t| t <= 2200), "poll phase {phase}: reopened at {t:?} µs");
    }
    // Today's owner (no republish timer): the view stays stale for the whole horizon.
    assert!(reopen_us(false, 0, 1000, 1000).is_none());
    // With the realized timer latencies (2.0 ms each) the reopen stays within L.
    let t = reopen_us(true, 0, 2000, 2060);
    assert!(t.is_some_and(|t| t <= 4_172 + 100), "reopened at {t:?} µs");
}

// ── 8. repair at β < 1 ──────────────────────────────────────────────────────

/// A 36/s call, cwnd 11 datagrams, srtt 100 ms, L = 4.2 ms: β ≈ 0.58. A first
/// send is handed when quinn's queue is empty; over 1 000 sources r_law = 0.25
/// sends exactly 250 repairs, each in its source's turn. A second source staged
/// before quinn has sent the first is refused.
#[test]
fn repair_at_beta_below_one() {
    let mut g = HandoffGate::new(true);
    g.fix_terms(Terms { tau_o: Some(0.002), l_o: 0.0, tau_p: Some(0.0022), l_p: 0.0 });
    let b = g.beta(0, &pin(0, 13_563, 100_000));
    assert!(b.is_some_and(|b| (b - 0.5775).abs() < 0.001), "β = {b:?}");
    assert!(g.admit(0, &pin(0, 13_563, 100_000)) && !g.admit(0, &pin(1, 13_563, 100_000)));
    let ins = [(0u32, pin(0, 13_563, 100_000))];
    let (mut debt, mut repairs) = (0.0, 0u64);
    for k in 0..1000u64 {
        assert!(!g.begin_turn(27_000 * (k + 1), true, false, &ins));
        assert!(g.check(true, &ins));
        g.note_first_send(0);
        debt += r_eff(0.25, g.spare_all());
        while debt >= 1.0 {
            debt -= 1.0;
            repairs += 1;
        }
    }
    assert_eq!(repairs, 250);
    // The second source waits no longer than L (≈ the republish timer plus the poll).
    let t = reopen_us_beta_below_one();
    assert!(t.is_some_and(|t| t <= 4_200 + 100), "second source handed after {t:?} µs");
}

fn reopen_us_beta_below_one() -> Option<i64> {
    let mut g = HandoffGate::new(true);
    g.fix_terms(Terms { tau_o: Some(0.002), l_o: 0.0, tau_p: Some(0.0022), l_p: 0.0 });
    let (staged, mut view_tx, mut last_full) = (1001u64, 1000u64, 0i64);
    let (mut next_poll, mut polling) = (0i64, true);
    let mut t = 0i64;
    while t < 100_000 {
        // quinn sent the first source at t = 0+ (its bucket was full)
        if t >= last_full + 2000 && owner_republishes(true, staged, 0, view_tx) {
            view_tx = 1001;
            last_full = t;
        }
        if polling && t >= next_poll {
            let i = PathIn { staged, staged_bytes: staged * S, tx: view_tx, cwnd: 13_563, qwin: 13_563, srtt_us: 100_000, ..Default::default() };
            let paused = g.begin_turn(t as u64, true, false, &[(0, i)]);
            if !paused {
                return Some(t);
            }
            polling = poll_armed(false, paused);
            next_poll = t + 2200;
        }
        t += 10;
    }
    None
}

// ── gauge and arm ───────────────────────────────────────────────────────────

/// The `[HOFF]` line prints `gate=on` when armed and `gate=off` otherwise, and
/// reads the gate's real state (the measured τ̂_o, not `(assumed)`).
#[test]
fn hoff_line_prints_gate_on_when_armed() {
    let mut gauge = HoffGauge::new(0);
    let mut g = HandoffGate::new(true);
    g.fix_terms(LEG1);
    let ins = [(0u32, pin(10, 280_000, 22_000))];
    g.begin_turn(1000, true, false, &ins);
    gauge.turn(1000, true, false, &ins);
    let l = gauge.line_gated(300_000, 40, false, Some(&mut g));
    assert!(l.contains(" gate=on "), "{l}");
    assert!(l.contains("tau_o=2.00ms(meas)") && !l.contains("(assumed)"), "{l}");
    assert!(l.contains(" beta=53.83 "), "{l}");
    let mut off = HoffGauge::new(0);
    off.turn(1000, true, false, &ins);
    let l = off.line_gated(300_000, 40, false, None);
    assert!(l.contains(" gate=off ") && l.contains("(assumed)"), "{l}");
}

// ── restricted picks ────────────────────────────────────────────────────────

/// An admitted path without Copa room (in flight at cwnd) still has room at
/// the hand-off: the restricted picks take it rather than an unadmitted path.
#[test]
fn restricted_picks_prefer_an_admitted_path_without_copa_room() {
    let mut s = Scheduler::new(Arc::new(WallClock));
    s.add_path(0);
    s.add_path(1);
    {
        let p = s.path_mut(1).unwrap();
        p.in_flight = p.cwnd;
    }
    assert_eq!(s.best_source_path_among(&|id| id == 1), Some(1));
    assert_eq!(s.best_repair_path_among(&|id| id == 1), Some(1));
    assert_eq!(s.best_source_path_among(&|_| false), s.best_source_path(), "no admitted path: the unrestricted pick");
}

// ── witness counters: what lets a first send past the bound ─────────────────

/// The classes at the anchor β = 53.83: below it the gate's own admit, at or
/// above it a leak unless a bypass site is tagged, and an undefined β is the
/// cold-start admit under any tag.
#[test]
fn first_send_classes_absolute() {
    let be = Some(53.83);
    assert_eq!(classify_first_send(53, be, None), FsClass::Gated);
    assert_eq!(classify_first_send(54, be, None), FsClass::Over);
    assert_eq!(classify_first_send(54, be, Some(Bypass::Defer)), FsClass::Bypass(Bypass::Defer));
    assert_eq!(classify_first_send(0, be, Some(Bypass::Timer)), FsClass::Gated, "a tag alone is not a leak");
    assert_eq!(classify_first_send(500, None, None), FsClass::Cold);
    assert_eq!(classify_first_send(500, None, Some(Bypass::Shutdown)), FsClass::Cold);
}

/// A turn in virtual time: the loop top opens the gate, a resend burst closes
/// it, then the deferred intake packet is staged anyway. The witness counts it
/// as a defer bypass, counts a gated first send as nothing, an untagged one
/// past the bound as `fs_over`, and resets with the row.
#[test]
fn deferred_packet_past_a_resend_burst_is_counted_as_a_bypass() {
    let mut g = HandoffGate::new(true);
    g.fix_terms(LEG1);
    let open = [(0u32, pin(10, 280_000, 22_000))];
    assert!(!g.begin_turn(1000, true, false, &open));
    g.note_first_send_at(0, &open[0].1); // B = 10 < 53.83
    let burst = pin(90, 280_000, 22_000); // resends staged: B = 90
    g.set_bypass(Some(Bypass::Defer));
    g.note_first_send_at(0, &burst);
    g.set_bypass(None);
    g.note_first_send_at(0, &burst); // untagged, past the bound
    g.note_first_send_at(0, &pin(500, 280_000, 0)); // no RTT: β undefined
    let r = g.take_row(0).unwrap();
    assert_eq!((r.fs_over, r.fs_bypass), (1, [1, 0, 0, 1]), "{r:?}");
    let r = g.take_row(0).unwrap();
    assert_eq!((r.fs_over, r.fs_bypass), (0, [0; 4]), "the window resets");
}

/// `ex_max` is the window's largest end-of-turn B − β with what was staged since
/// the gate last admitted on the path, as `(total, first sends, exempt)`: the
/// rest of the excess is carry-in or drain, never booked to a kind. `ex_n` counts
/// the window's exempt datagrams by kind (control, resend, repair).
#[test]
fn end_of_turn_excess_splits_into_first_and_exempt_parts() {
    let mut g = HandoffGate::new(true);
    g.fix_terms(LEG1);
    let at = |b: u64, kinds: [u64; 4]| [(0u32, PathIn { kinds, ..pin(b, 280_000, 22_000) })];
    g.begin_turn(1000, true, false, &at(40, [0; 4])); // open: B = 40 < 53.83
    // This turn staged 5 first sends, 2 resends, 3 repairs, 4 control: B = 100.
    g.end_turn(&at(100, [5, 2, 3, 4]));
    let (t, f, x) = g.take_row(0).unwrap().ex_max.unwrap();
    assert!((t - 46.17).abs() < 0.01 && f == 5.0 && x == 9.0, "{t} {f} {x}");
    // Closed turns follow; a resend burst lands three turns after the last open one.
    g.begin_turn(2000, true, false, &at(100, [5, 2, 3, 4]));
    g.end_turn(&at(100, [5, 2, 3, 4]));
    g.begin_turn(3000, true, false, &at(120, [5, 12, 3, 4]));
    g.end_turn(&at(140, [5, 12, 3, 4]));
    let r = g.take_row(0).unwrap();
    let (t, f, x) = r.ex_max.unwrap();
    // Booked since the last OPEN turn's start (kinds [0; 4]): 5 first sends and 19 exempt;
    // the burst is not read as first sends, and the carry-in (86.17 - 24) stays unbooked.
    assert!((t - 86.17).abs() < 0.01 && f == 5.0 && x == 19.0, "{r:?}");
    assert_eq!((r.ex_n, r.fs_n), ([0, 10, 0], 0), "control, resend, repair; first sends");
    let r = g.take_row(0).unwrap();
    assert_eq!((r.ex_max, r.ex_n), (None, [0; 3]), "the window resets");
}

/// β's spread over the window's per-turn values, not the one end sample: β is
/// proportional to cwnd here, 20 turns at cwnd k·10 000 → p5 the 1st, p50 the
/// 10th, p95 the 19th.
#[test]
fn beta_spread_covers_every_turn_of_the_window() {
    let mut g = HandoffGate::new(true);
    g.fix_terms(LEG1);
    for k in 1..=20u64 {
        g.begin_turn(k * 1000, false, false, &[(0, pin(0, k * 10_000, 22_000))]);
    }
    let r = g.take_row(0).unwrap();
    let b = |k: u64| g.beta(0, &pin(0, k * 10_000, 22_000)).unwrap();
    let (p5, p50, p95) = r.beta_q.unwrap();
    assert!((p5 - b(1)).abs() < 1e-9 && (p50 - b(10)).abs() < 1e-9 && (p95 - b(19)).abs() < 1e-9, "{r:?}");
    assert_eq!(r.beta, Some(b(20)), "the end sample is still the last turn's");
}

/// `pend_max` = staged − handed to quinn − dropped, at turn starts; `own_gap_max`
/// is the owner's longest drain gap the end-of-turn readings carried.
#[test]
fn pending_and_owner_gap_maxima() {
    let mut g = HandoffGate::new(true);
    g.fix_terms(LEG1);
    let at = |staged: u64, handed: u64, dropped: u64, gap: u64| {
        [(0u32, PathIn { staged, handed, dropped, own_gap_us: gap, ..pin(0, 280_000, 22_000) })]
    };
    g.begin_turn(1000, false, false, &at(100, 90, 4, 0)); // pending 6
    g.end_turn(&at(100, 90, 4, 3_000));
    g.begin_turn(2000, false, false, &at(300, 100, 0, 0)); // pending 200
    g.end_turn(&at(300, 100, 0, 18_000));
    g.begin_turn(3000, false, false, &at(310, 305, 0, 0)); // pending 5
    g.end_turn(&at(310, 305, 0, 500));
    let r = g.take_row(0).unwrap();
    assert_eq!((r.pend_max, r.own_gap_max_us), (200, 18_000), "{r:?}");
    let r = g.take_row(0).unwrap();
    assert_eq!((r.pend_max, r.own_gap_max_us), (0, 0));
}

/// The owner's gap is read between two `crate::clock` instants: absolute at
/// 3.5 ms and 0 for the first drain.
#[test]
fn owner_gap_is_an_interval_between_clock_reads() {
    let t0 = crate::clock::now();
    assert_eq!(own_gap_us(None, t0), 0);
    assert_eq!(own_gap_us(Some(t0), t0 + std::time::Duration::from_micros(3500)), 3500);
    assert_eq!(own_gap_us(Some(t0 + std::time::Duration::from_secs(1)), t0), 0, "never negative");
}

/// The staged-by-kind index is the `[HOFF]` mapping: a data datagram defaults
/// to repair, and the engine seams label first sends and resends.
#[test]
fn send_kinds_and_their_labels_at_the_seams() {
    assert_eq!(SendKind::default(), SendKind::Repair);
    assert_eq!(
        [SendKind::First as usize, SendKind::Resend as usize, SendKind::Repair as usize, SendKind::Ctrl as usize],
        [0, 1, 2, 3]
    );
    let m = flat(include_str!("mod.rs"));
    let e = flat(include_str!("emit_source.rs"));
    let p = flat(include_str!("sender_phases.rs"));
    let o = flat(include_str!("../transport/io_owner.rs"));
    assert!(e.contains("st.tx.mark(crate::transport::io_owner::SendKind::First)") && e.contains("note_first_send_live(source_path)"));
    assert!(p.contains("st.tx.mark(crate::transport::io_owner::SendKind::Resend)"));
    assert!(m.contains("st.tx.mark(crate::transport::io_owner::SendKind::Resend)"));
    for b in ["Defer", "Timer", "Shutdown"] {
        assert!(m.contains(&format!("set_bypass(Some(hoff_gate::Bypass::{b}))")), "{b} site is tagged");
    }
    assert!(m.contains("hg.end_turn_live()"), "the turn ends with the sample");
    assert!(o.contains("own_gap_max_us.fetch_max(own_gap_us(last_drain,t),Relaxed)"), "the owner records its gap");
    assert!(!o.contains("last_drain=Some(Instant::now())"), "owner timing reads go through crate::clock");
}

/// With the gate on the line carries the witness fields; with it off it does
/// not (arm-off output is unchanged).
#[test]
fn hoff_line_prints_the_witness_fields_only_when_armed() {
    let mut gauge = HoffGauge::new(0);
    let mut g = HandoffGate::new(true);
    g.fix_terms(LEG1);
    let ins = [(0u32, PathIn { kinds: [0, 1, 2, 3], ..pin(10, 280_000, 22_000) })];
    g.begin_turn(1000, true, false, &ins);
    gauge.turn(1000, true, false, &ins);
    g.set_bypass(Some(Bypass::Timer));
    g.note_first_send_at(0, &pin(90, 280_000, 22_000));
    g.end_turn(&[(0u32, PathIn { kinds: [0, 1, 2, 3], own_gap_us: 7_000, ..pin(100, 280_000, 22_000) })]);
    let l = gauge.line_gated(300_000, 40, false, Some(&mut g));
    for f in [
        " fs_over=0 ",
        " fs_bypass=0/1/0/0 ",
        " ex_max=46/0/0 ",
        " ex_n=0/0/0 fs_n=0 ",
        " beta_q=53.83/53.83/53.83 ",
        " own_gap_max=7000us ",
        " pend_max=1010]",
    ] {
        assert!(l.contains(f), "missing {f}: {l}");
    }
    let mut off = HoffGauge::new(0);
    off.turn(1000, true, false, &ins);
    let l = off.line_gated(300_000, 40, false, None);
    for f in ["fs_over=", "fs_bypass=", "ex_max=", "ex_n=", "beta_q=", "own_gap_max=", "pend_max="] {
        assert!(!l.contains(f), "{f} printed with the gate off: {l}");
    }
}
