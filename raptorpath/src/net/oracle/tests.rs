//! Host gate for the §43 oracle arm (status §43, tests (a)-(g), plus the log
//! contract). Expected values are written out by hand from the paper block,
//! never read back from the engine.

use super::*;
use crate::control::fec_rate::ProtocolHint;
use crate::gates::RuntimeGates;
use crate::net::channel_set::store_cap_pool_inputs;
use crate::net::report::SenderTeardownGauges;
use crate::net::sender_phases::{refresh_store_cap, StoreCapCtx, StoreCapState};
use crate::net::sender_policy::{SenderPolicy, STORE_CAP_FLOOR};
use crate::net::store_cap::{delta_budget_b, pooled_store_cap, DeltaCapGauge, SumCapGauge};
use crate::scheduler::MockClock;
use std::sync::Arc;
use std::time::Duration;

const KNEE: usize = 2048;

/// Installs a fresh gauge for this test's thread (the gate-on path without
/// flipping the process-global `OnceLock`) and removes it on drop, panic or not.
struct TestGauge(&'static OracleGauge);
impl TestGauge {
    fn install() -> Self {
        let g: &'static OracleGauge = Box::leak(Box::new(OracleGauge::new()));
        TEST_GAUGE.with(|c| c.set(Some(g)));
        TestGauge(g)
    }
}
impl Drop for TestGauge {
    fn drop(&mut self) {
        TEST_GAUGE.with(|c| c.set(None));
    }
}

fn gates(law: bool, diag: bool, l: &[f64], hold: Option<(usize, f64)>) -> OracleGates {
    OracleGates { law, diag, l_dgps: l.to_vec(), hold, gain: None, err: None }
}

/// One path whose RTprop is `rtt_us` (a sample fed through the path itself,
/// so it also reaches the installed gauge, as in the engine).
fn sched_with_rtt(rtt_us: Option<u64>) -> Scheduler {
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new(clock);
    sched.add_path(0);
    if let Some(us) = rtt_us {
        sched.path_mut(0).unwrap().record_rtt_sample(Duration::from_micros(us));
    }
    sched
}

/// Close a window holding exactly `samples` (µs) and return S in µs.
fn close_with(g: &OracleGauge, samples: &[u32]) -> CloseRec {
    for s in samples {
        g.note_rtt(0, *s);
    }
    g.close_path(0)
}

fn decide_at(
    og: &OracleGates,
    g: &OracleGauge,
    sched: &Scheduler,
    b: f64,
    t_s: f64,
) -> (CapDecision, usize) {
    decide(og, g, sched, b, STORE_CAP_FLOOR, KNEE, t_s)
}

// ── (b) the formula's absolute values ────────────────────────────────────

/// Bulk, L 9 773, RTprop 7.0 ms, S 5.4 ms: ask 133.3, n = 134. L 97 733,
/// RTprop 2.60 ms, S 2.60 ms: ask 559.0 (559.03), n = 560. The clamp gives 10
/// and 2 048. Written from the paper: 1.1 · 9773 · 0.0124 = 133.3037 and
/// 1.1 · 97733 · 0.0052 = 559.0328.
#[test]
fn oracle43_b_absolute_values() {
    let q = codel_setpoint_q(delta_budget_b(ProtocolHint::Bulk));
    assert!((q - 0.10).abs() < 1e-12, "Bulk q = {q}");
    let a1 = oracle_ask(q, &[OracleTerm { l_dgps: 9773.0, rtprop_s: 0.0070, s_s: 0.0054 }]);
    assert!((a1 - 133.3037).abs() < 1e-3, "c2 ask {a1}");
    assert_eq!(oracle_cap(a1, STORE_CAP_FLOOR, 0, KNEE).n, 134);
    let a2 = oracle_ask(q, &[OracleTerm { l_dgps: 97733.0, rtprop_s: 0.0026, s_s: 0.0026 }]);
    assert!((a2 - 559.0328).abs() < 1e-3, "c1 ask {a2}");
    assert_eq!(oracle_cap(a2, STORE_CAP_FLOOR, 0, KNEE).n, 560);
    // The clamp.
    let lo = oracle_cap(0.4, STORE_CAP_FLOOR, 0, KNEE);
    assert_eq!((lo.n, lo.floor_bind, lo.pin_bind), (10, true, false));
    let hi = oracle_cap(1.0e9, STORE_CAP_FLOOR, 0, KNEE);
    assert_eq!((hi.n, hi.floor_bind, hi.pin_bind), (2048, false, true));
    let mid = oracle_cap(a1, STORE_CAP_FLOOR, 0, KNEE);
    assert!(!mid.floor_bind && !mid.pin_bind && !mid.hold_bind);
}

/// The same two values through `decide`: the live path's Copa RTprop, S from
/// the sampler's closed window, L from the gate, q from δ. This is the wiring
/// the formula test above does not touch.
#[test]
fn oracle43_b_decide_routes_rtprop_s_l_and_q() {
    let tg = TestGauge::install();
    let sched = sched_with_rtt(Some(7000));
    // The path's own sample (7000) is already in the window: S = 12400 − 7000.
    let rec = close_with(tg.0, &[12400, 12400]);
    assert_eq!((rec.n_k, rec.tmin_us, rec.tp50_us, rec.s_us), (3, Some(7000), Some(12400), 5400));
    let og = gates(true, false, &[9773.0], None);
    let (d, h) = decide_at(&og, tg.0, &sched, delta_budget_b(ProtocolHint::Bulk), 20.0);
    assert!((d.ask - 133.3037).abs() < 1e-3, "{}", d.ask);
    assert_eq!((d.n, h), (134, 0));

    let tg2 = TestGauge::install();
    let sched2 = sched_with_rtt(Some(2600));
    close_with(tg2.0, &[5200, 5200]);
    let og2 = gates(true, false, &[97733.0], None);
    let (d2, _) = decide_at(&og2, tg2.0, &sched2, delta_budget_b(ProtocolHint::Bulk), 20.0);
    assert!((d2.ask - 559.0328).abs() < 1e-3, "{}", d2.ask);
    assert_eq!(d2.n, 560);
}

// ── the diagnostic gain (RWM_ORACLE_GAIN) ────────────────────────────────

/// A set gain replaces (1 + q): L 9 773, RTprop 7.0 ms, S 5.4 ms at gain 2.0 gives
/// 2 · 9773 · 0.0124 = 242.3704 -> n = 243 (the default, Bulk q, gives 134). Unset
/// is the default exactly; the parse is strict; the echo declares it.
#[test]
fn oracle43_gain_replaces_one_plus_q() {
    let tg = TestGauge::install();
    let sched = sched_with_rtt(Some(7000));
    close_with(tg.0, &[12400, 12400]);
    let b = delta_budget_b(ProtocolHint::Bulk);
    let mut og = gates(true, false, &[9773.0], None);
    let (d0, _) = decide_at(&og, tg.0, &sched, b, 20.0);
    assert_eq!(d0.n, 134, "unset: the default formula, unchanged");
    og.apply_gain(Some("2.0"));
    assert_eq!(og.gain, Some(2.0));
    let (d, _) = decide_at(&og, tg.0, &sched, b, 20.0);
    assert!((d.ask - 242.3704).abs() < 1e-3, "{}", d.ask);
    assert_eq!(d.n, 243);
    // The gain replaces the multiplier: δ no longer moves the ask.
    assert_eq!(decide_at(&og, tg.0, &sched, 0.5, 20.0).0.n, 243);
    // Gain 1.1 is the Bulk default.
    og.apply_gain(Some("1.1"));
    assert_eq!(decide_at(&og, tg.0, &sched, 0.5, 20.0).0.n, 134);
    // Strict parse, and the echo declares it.
    for bad in ["0", "-1", "x", "inf", "NaN", ""] {
        let mut g = gates(true, false, &[9773.0], None);
        g.apply_gain(Some(bad));
        assert!(g.check(1, KNEE, true, true).is_err(), "{bad:?}");
    }
    let mut g = gates(true, false, &[9773.0], None);
    assert!(g.echo().unwrap().ends_with("gain=q"));
    g.apply_gain(Some("2"));
    assert!(g.echo().unwrap().ends_with("gain=2"), "{:?}", g.echo());
    let mut m = gates(false, true, &[9773.0], None);
    m.apply_gain(Some("2"));
    assert!(m.echo().unwrap().contains("gain=2") && !m.echo().unwrap().contains("oracle store law ACTIVE"));
}

// ── (c) a 30 000-sample window kept whole ────────────────────────────────

/// The window is a permutation of 1..=30 000 (i·7 mod 30 000 + 1, 7 coprime to
/// 30 000). Nearest-rank median: index round(29 999 · 0.5) = 15 000 (half away
/// from zero), whose value is 15 001; min 1. Nothing is capped (the
/// `[LOOPDIAG]` instrument would have kept 8 192), and Σ n_k equals the
/// offered count across windows.
#[test]
fn oracle43_c_a_30000_sample_window_is_kept_whole() {
    let g = OracleGauge::new();
    for i in 0..30_000u32 {
        g.note_rtt(0, (i * 7) % 30_000 + 1);
    }
    let w1 = g.close_path(0);
    assert_eq!(w1.n_k, 30_000, "the window was cut");
    assert!(w1.n_k > 8192);
    assert_eq!((w1.tmin_us, w1.tp50_us), (Some(1), Some(15_001)));
    assert_eq!(w1.s_us, 15_000);
    assert_eq!((w1.offered, w1.cum_nk), (30_000, 30_000));
    // The median is a real sample with exactly its value's rank below it.
    let p50 = w1.tp50_us.unwrap();
    assert_eq!(p50, 15_001, "a permutation of 1..=N: the rank-15 001 sample is 15 001");
    // A second, short window: Σ n_k == offered at the close, per path.
    for v in 1..=17u32 {
        g.note_rtt(0, v);
    }
    let w2 = g.close_path(0);
    assert_eq!(w2.n_k, 17);
    assert_eq!((w2.tmin_us, w2.tp50_us), (Some(1), Some(9)), "round(16·0.5) = 8 -> value 9");
    assert_eq!(w2.cum_nk, 30_017);
    assert_eq!(w2.offered, w2.cum_nk, "Σ n_k must equal the offered count");
    // Another path is independent.
    g.note_rtt(1, 500);
    let w3 = g.close_path(1);
    assert_eq!((w3.n_k, w3.offered, w3.cum_nk), (1, 1, 1));
}

/// The sampler is fed by the path's own sample entry point, the series Copa
/// consumes.
#[test]
fn oracle43_c_record_rtt_sample_feeds_the_gauge() {
    let tg = TestGauge::install();
    let mut sched = sched_with_rtt(None);
    let p = sched.path_mut(0).unwrap();
    for us in [3000u64, 4000, 5000] {
        p.record_rtt_sample(Duration::from_micros(us));
    }
    let w = tg.0.close_path(0);
    assert_eq!((w.n_k, w.tmin_us, w.tp50_us), (3, Some(3000), Some(4000)));
    assert_eq!(w.offered, 3);
}

// ── (d) the hold handover ────────────────────────────────────────────────

/// n = max(ask, H) while t < t_h and n = ask after; ask is logged in both.
#[test]
fn oracle43_d_hold_handover() {
    let tg = TestGauge::install();
    let sched = sched_with_rtt(Some(7000));
    close_with(tg.0, &[12400, 12400]);
    let b = delta_budget_b(ProtocolHint::Bulk);
    let og = gates(true, false, &[9773.0], Some((1536, 15.0)));
    // ask 133.3 < H: the hold binds.
    let (d, h) = decide_at(&og, tg.0, &sched, b, 14.999);
    assert_eq!((d.n, h, d.hold_bind, d.floor_bind), (1536, 1536, true, false));
    assert!((d.ask - 133.3037).abs() < 1e-3, "ask is logged during the hold: {}", d.ask);
    // The handover is at t_h exactly (t < t_h).
    let (d, h) = decide_at(&og, tg.0, &sched, b, 15.0);
    assert_eq!((d.n, h, d.hold_bind), (134, 0, false));
    assert!((d.ask - 133.3037).abs() < 1e-3, "ask is logged after the hold: {}", d.ask);
    // An ask above H wins during the hold (max, not an override).
    assert_eq!(oracle_cap(1800.2, STORE_CAP_FLOOR, 1536, KNEE).n, 1801);
    assert!(!oracle_cap(1800.2, STORE_CAP_FLOOR, 1536, KNEE).hold_bind);
    // h(t) itself.
    assert_eq!(hold_h(Some((1536, 15.0)), 0.0), 1536);
    assert_eq!(hold_h(Some((1536, 15.0)), 15.0), 0);
    assert_eq!(hold_h(None, 0.0), 0);
    // The gauge carries ask, h and the hold bind fraction to the [OCAP] line.
    tg.0.note_refresh(1536, &oracle_cap(133.3, STORE_CAP_FLOOR, 1536, KNEE), 1536);
    let rf = tg.0.drain_refresh();
    assert_eq!((rf.n, rf.h, rf.hold, rf.refreshes), (1536, 1536, 1, 1));
}

// ── (e) the configuration checks ─────────────────────────────────────────

/// A mis-sized L list, H above N·knee, a missing RWM_DIAG, a seat the law
/// cannot run on, and every parse error each refuse to start.
#[test]
fn oracle43_e_configuration_checks_refuse() {
    let p = OracleGates::parse;
    let ok = |g: &OracleGates, n: usize| g.check(n, KNEE, true, true);
    // One path, one L.
    assert_eq!(ok(&p(Some("oracle"), Some("9773"), None, None), 1), Ok(()));
    // The length must equal the path count, both ways.
    let e = ok(&p(Some("oracle"), Some("9773,9773"), None, None), 1).unwrap_err();
    assert!(e.contains("2 entries") && e.contains("1 paths"), "{e}");
    assert!(ok(&p(Some("oracle"), Some("9773"), None, None), 2).is_err());
    // The law without an L list is refused (0 entries).
    assert!(ok(&p(Some("oracle"), None, None, None), 1).is_err());
    // MAIN's readout needs the list too.
    assert!(ok(&p(None, None, None, Some("1")), 1).is_err());
    assert_eq!(ok(&p(None, Some("9773"), None, Some("1")), 1), Ok(()));
    // H <= N·knee.
    assert_eq!(ok(&p(Some("oracle"), Some("9773"), Some("2048:15"), None), 1), Ok(()));
    let e = ok(&p(Some("oracle"), Some("9773"), Some("2049:15"), None), 1).unwrap_err();
    assert!(e.contains("N*knee"), "{e}");
    assert_eq!(ok(&p(Some("oracle"), Some("9773,9773"), Some("4096:15"), None), 2), Ok(()));
    assert!(ok(&p(Some("oracle"), Some("9773,9773"), Some("4097:15"), None), 2).is_err());
    // RWM_DIAG is required (windows close on its tick); the seat must be the plain dynamic cap.
    let g = p(Some("oracle"), Some("9773"), None, None);
    assert!(g.check(1, KNEE, false, true).unwrap_err().contains("RWM_DIAG"));
    assert!(g.check(1, KNEE, true, false).is_err());
    // A hold without the law is meaningless.
    assert!(ok(&p(None, Some("9773"), Some("1536:15"), Some("1")), 1).is_err());
    // Parse errors survive to the check.
    for bad in [
        p(Some("Oracle"), Some("9773"), None, None),
        p(Some("oracle"), Some("9773,x"), None, None),
        p(Some("oracle"), Some("0"), None, None),
        p(Some("oracle"), Some("9773"), Some("1536"), None),
        p(Some("oracle"), Some("9773"), Some("0:15"), None),
        p(Some("oracle"), Some("9773"), Some("1536:-1"), None),
        p(Some("oracle"), Some("9773"), None, Some("yes")),
    ] {
        assert!(ok(&bad, 1).is_err(), "{bad:?}");
    }
    // Everything off is inert, whatever else is in the environment.
    assert_eq!(p(None, None, None, None), OracleGates::default());
    assert_eq!(ok(&OracleGates::default(), 3), Ok(()));
    assert_eq!(ok(&p(Some("pooled"), None, None, None), 3), Ok(()));
}

/// The check is wired into the engine start (`run_impl`), before anything is
/// bound: a source-level pin, since the gates are process-global.
#[test]
fn oracle43_e_check_is_wired_into_run_impl() {
    let src = include_str!("../mod.rs");
    let flat: String = src.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        flat.contains("gates .oracle .check( config.bind_addrs.len(), gates.store_path_pool, gates.diag,"),
        "run_impl no longer refuses a mis-configured oracle arm"
    );
    let at_check = flat.find("gates .oracle .check(").unwrap();
    let at_bind = flat.find("&config.bind_addrs,").unwrap();
    assert!(at_check < at_bind, "the check must run before the transport binds");
}

// ── (f) δ continuity and monotonicity ────────────────────────────────────

/// n is continuous and monotone in b through every named point (±2 % nudges),
/// and the fine sweep has no step. At Bulk q saturates (b = 2 is the dial's
/// end), so the +2 % nudge is flat there: non-decreasing, not increasing.
/// Absolute at the named points: L = 97 733, RTprop = S = 2.6 ms gives
/// 508.2116 · (1 + q) with q = 1/20, 1/15, 1/10.
#[test]
fn oracle43_f_n_is_continuous_and_monotone_in_delta() {
    let tg = TestGauge::install();
    let sched = sched_with_rtt(Some(2600));
    close_with(tg.0, &[5200, 5200]);
    let og = gates(true, false, &[97733.0], None);
    let n_at = |b: f64| decide_at(&og, tg.0, &sched, b, 20.0).0.n;
    let base: f64 = 97733.0 * 0.0052;
    for (hint, q) in [
        (ProtocolHint::Realtime, 1.0 / 20.0),
        (ProtocolHint::Auto, 1.0 / 15.0),
        (ProtocolHint::Bulk, 1.0 / 10.0),
    ] {
        let b = delta_budget_b(hint);
        let (lo, mid, hi) = (n_at(b * 0.98), n_at(b), n_at(b * 1.02));
        assert_eq!(mid, ((1.0 + q) * base).ceil() as usize, "{hint:?}: absolute value");
        assert!(lo <= mid && mid <= hi, "{hint:?}: not monotone ({lo},{mid},{hi})");
        assert!(mid - lo <= 2 && hi - mid <= 2, "{hint:?}: a step at the preset ({lo},{mid},{hi})");
    }
    let (rt, au, bu) = (
        n_at(delta_budget_b(ProtocolHint::Realtime)),
        n_at(delta_budget_b(ProtocolHint::Auto)),
        n_at(delta_budget_b(ProtocolHint::Bulk)),
    );
    assert_eq!((rt, au, bu), (534, 543, 560));
    assert_eq!(n_at(2.0 * 1.02), n_at(2.0), "q saturates at Bulk: the +2 % nudge is flat");
    let mut prev = n_at(0.5);
    for i in 1..=150 {
        let b = 0.5 + 1.5 * (i as f64 / 150.0);
        let n = n_at(b);
        assert!(n >= prev, "b={b}: not monotone");
        assert!(n - prev <= 1, "b={b}: a step of {} symbols", n - prev);
        prev = n;
    }
}

// ── (g) boot ─────────────────────────────────────────────────────────────

/// n = floor before any sample; from the first sample to the first close
/// (S = 0), n = clamp((1 + q)·L·RTprop); the close then adds S; an empty
/// window leaves S as it was.
#[test]
fn oracle43_g_boot() {
    let tg = TestGauge::install();
    let mut sched = sched_with_rtt(None);
    let og = gates(true, false, &[9773.0], None);
    let b = delta_budget_b(ProtocolHint::Bulk);
    let (d, _) = decide_at(&og, tg.0, &sched, b, 0.0);
    assert_eq!((d.n, d.floor_bind), (STORE_CAP_FLOOR, true), "before any sample n = floor, not the boot cap");
    assert_eq!(STORE_CAP_FLOOR, 10);
    // First sample (7.0 ms), window not yet closed: S = 0.
    sched.path_mut(0).unwrap().record_rtt_sample(Duration::from_micros(7000));
    let (d, _) = decide_at(&og, tg.0, &sched, b, 0.1);
    assert!((d.ask - 1.1 * 9773.0 * 0.007).abs() < 1e-6, "{}", d.ask); // 75.25
    assert_eq!(d.n, 76);
    // A one-sample window closes with S = 0: n unchanged.
    let w = tg.0.close_path(0);
    assert_eq!((w.n_k, w.s_us), (1, 0));
    assert_eq!(decide_at(&og, tg.0, &sched, b, 0.3).0.n, 76);
    // A real window: S = p50 − min = 5400 µs -> ask 133.3 -> 134.
    close_with(tg.0, &[7000, 12400, 12400]);
    assert_eq!(decide_at(&og, tg.0, &sched, b, 0.5).0.n, 134);
    // An empty window keeps S.
    let w = tg.0.close_path(0);
    assert_eq!((w.n_k, w.tmin_us, w.tp50_us, w.s_us), (0, None, None, 5400));
    assert_eq!(decide_at(&og, tg.0, &sched, b, 0.75).0.n, 134);
}

// ── the log contract ─────────────────────────────────────────────────────

/// The exact `[OCAP]` line, one literal shared with the harness parser test
/// (`tools/l1/test_place43_parse.py`, same string). A change here must change
/// both.
pub(crate) const OCAP_FIXTURE: &str = "[OCAP] t=12.5s p0 law=oracle n=157 ask=156.42 h=0 \
rtprop_us=7012 tmin_us=7100 tp50_us=15200 s_us=8100 brake_us=88 n_k=2412 offered=30120 r_k=16.45 \
floor=0.000 pin=0.000 hold=0.000 wbind=0 nmin=150 nmax=161 refreshes=50 q=0.1000";

#[test]
fn oracle43_ocap_line_is_the_logged_contract() {
    let rf = RefreshWin {
        refreshes: 50,
        floor: 0,
        pin: 0,
        hold: 0,
        n_min: 150,
        n_max: 161,
        n: 157,
        ask: 156.42,
        h: 0,
    };
    let w = CloseRec {
        n_k: 2412,
        tmin_us: Some(7100),
        tp50_us: Some(15200),
        s_us: 8100,
        offered: 30120,
        cum_nk: 30120,
    };
    assert_eq!(format_ocap(12.5, 0, true, 0.1, &rf, &w, Some(7012)), OCAP_FIXTURE);
    // An empty window prints `-`, never a zero that reads as a measurement;
    // wbind fires when the window is shorter than Tp50/q (a 300 ms Tp50).
    let empty = CloseRec { n_k: 0, tmin_us: None, tp50_us: None, s_us: 0, offered: 0, cum_nk: 0 };
    let l = format_ocap(0.25, 0, false, 0.1, &rf, &empty, None);
    assert!(l.contains(" law=shadow ") && l.contains("rtprop_us=- tmin_us=- tp50_us=- "), "{l}");
    assert!(l.contains("brake_us=- n_k=0 ") && l.contains(" r_k=- ") && l.contains(" wbind=0 "), "{l}");
    let slow = CloseRec { tmin_us: Some(100_000), tp50_us: Some(300_000), n_k: 9, ..w };
    let l = format_ocap(2.0, 0, true, 0.1, &rf, &slow, Some(90_000));
    assert!(l.contains(" wbind=1 ") && l.contains(" r_k=0.83 ") && l.contains("brake_us=10000"), "{l}");
}

/// The `[DIAG]` tick (`diag::report` calls `close_tick`): every registered path's
/// window closes once, in path-id order (paths added 1 then 0), the refresh
/// accumulators drain once (both lines carry the same n / ask / bind window),
/// S carries across an empty window, and the next tick starts a fresh refresh
/// window with the last n still in force.
#[test]
fn oracle43_tick_closes_every_path_once_and_drains_the_refresh_once() {
    let tg = TestGauge::install();
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new(clock);
    sched.add_path(1);
    sched.add_path(0);
    sched.path_mut(0).unwrap().record_rtt_sample(Duration::from_micros(7000));
    sched.path_mut(1).unwrap().record_rtt_sample(Duration::from_micros(3000));
    tg.0.note_rtt(0, 12400);
    tg.0.note_rtt(1, 3500);
    tg.0.note_rtt(1, 3500);
    tg.0.note_refresh(201, &oracle_cap(200.4, STORE_CAP_FLOOR, 0, 2 * KNEE), 0);
    tg.0.note_refresh(250, &oracle_cap(250.0, STORE_CAP_FLOOR, 0, 2 * KNEE), 0);
    let b = delta_budget_b(ProtocolHint::Bulk);
    let l = close_tick(tg.0, &sched, 12.5, b, true);
    assert_eq!(l.len(), 2);
    assert_eq!(
        l[0],
        "[OCAP] t=12.5s p0 law=oracle n=250 ask=250.00 h=0 rtprop_us=7000 tmin_us=7000 tp50_us=12400 s_us=5400 \
         brake_us=0 n_k=2 offered=2 r_k=20.16 floor=0.000 pin=0.000 hold=0.000 wbind=0 nmin=201 nmax=250 \
         refreshes=2 q=0.1000"
    );
    assert!(l[1].starts_with("[OCAP] t=12.5s p1 law=oracle n=250 ask=250.00 h=0 rtprop_us=3000 tmin_us=3000 tp50_us=3500 s_us=500 "), "{}", l[1]);
    assert!(l[1].contains(" n_k=3 offered=3 ") && l[1].contains(" nmin=201 nmax=250 refreshes=2 "), "{}", l[1]);
    // Next tick: no refresh happened, no sample arrived. n carries, the window is empty, S carries, offered stays.
    let l = close_tick(tg.0, &sched, 12.75, b, true);
    assert!(l[0].contains(" n=250 ask=250.00 ") && l[0].contains(" tmin_us=- tp50_us=- s_us=5400 brake_us=- n_k=0 offered=2 r_k=- "), "{}", l[0]);
    assert!(l[0].contains(" nmin=250 nmax=250 refreshes=0 "), "{}", l[0]);
}

// ── (a) default off is bit-identical ─────────────────────────────────────

fn warm_one_path() -> Scheduler {
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new(clock.clone());
    sched.add_path(0);
    let p = sched.path_mut(0).unwrap();
    p.record_rtt_sample(Duration::from_millis(2));
    let step = Duration::from_micros(41);
    for seq in 0..4000u64 {
        p.on_src_sent(seq, false);
        if seq >= 72 {
            p.on_src_delivered_seq(seq - 72);
        }
        clock.advance(step);
    }
    assert!(p.copa_bdp_anchor().is_some(), "anchor must be warm");
    sched
}

fn policy() -> SenderPolicy {
    let mut g = RuntimeGates::resolve();
    g.store_paths = true;
    g.sum_cap = true;
    g.delta_cap = true;
    g.three_term = false;
    g.composed_cap = false;
    g.pool_anchor = false;
    g.honest_cap = false;
    g.plain_rs = false;
    g.copa_feed = false;
    g.store_override = None;
    g.store_env_set = false;
    g.derived_sweep = false;
    g.store_gain = 2.0;
    SenderPolicy::resolve(&g, 1200, ProtocolHint::Bulk, true, false, false, false)
}

/// One refresh; returns the cap, the `[SUMCAP]` line and the pooled inputs.
fn refresh(og: OracleGates, sched: &mut Scheduler) -> (usize, String, f64) {
    let pol = policy();
    assert!(pol.plain_dyn_cap);
    let mut gates = RuntimeGates::resolve();
    gates.oracle = og;
    let mut scs = StoreCapState::new(&pol);
    let mut sumcap = SumCapGauge::new(
        pol.sum_cap, pol.store_cap_floor, pol.store_path_pool, pol.delta_cap, pol.delta_b);
    let mut dcap = DeltaCapGauge::new(
        pol.delta_cap, pol.store_cap_floor, pol.store_path_pool, pol.sum_cap, pol.delta_b);
    let mut ccap = SenderTeardownGauges::new(false, pol.store_cap_floor);
    let (bdp, _) = store_cap_pool_inputs(sched, false);
    let copa_feed: Option<Arc<crate::net::copa_feed::CopaFeed>> = None;
    refresh_store_cap(
        &mut scs,
        StoreCapCtx {
            pol: &pol,
            gates: &gates,
            scheduler: sched,
            copa_feed: &copa_feed,
            sumcap: &mut sumcap,
            dcap: &mut dcap,
            ccap: &mut ccap,
            diag_start_us: crate::net::now_us(),
        },
    );
    (scs.dyn_store_cap, sumcap.sumcap_line(), bdp)
}

/// (a) With every new env unset the cap is the shipped pooled law and the
/// gauge does not exist. This is a pin: it also holds on `843a1bb`, where it
/// does not compile, so it cannot be red against the base. Beside it, the same
/// scenario with only `RWM_ORACLE_DIAG=1` leaves the cap unchanged (the
/// readout is observation only) while the gauge records the counterfactual ask.
#[test]
fn oracle43_a_default_off_is_bit_identical() {
    assert!(gauge().is_none(), "no gauge with both gates off");
    let og0 = OracleGates::default();
    assert!(!og0.active() && og0.echo().is_none());
    let mut sched = warm_one_path();
    let (cap, line, bdp) = refresh(og0, &mut sched);
    let pol = policy();
    let want = pooled_store_cap(
        true, true, true, pol.delta_b, 1, bdp, 2.0, pol.store_cap_floor, pol.store_path_pool,
    )
    .unwrap();
    assert_eq!(cap, want);
    assert_eq!(cap, ((1.1 * bdp).ceil() as usize), "ceil((1+q)·bdp) at Bulk");
    assert!(line.contains(" eng=1/1 "), "[SUMCAP] engaged: {line}");

    // Readout only: same cap, same [SUMCAP], and the counterfactual is logged.
    let tg = TestGauge::install();
    let mut sched = warm_one_path();
    let (cap2, line2, _) = refresh(gates(false, true, &[9773.0], None), &mut sched);
    assert_eq!(cap2, cap, "RWM_ORACLE_DIAG alone must not move the cap");
    assert_eq!(line2, line);
    let rf = tg.0.drain_refresh();
    assert_eq!((rf.refreshes, rf.n, rf.h), (1, cap, 0));
    assert!((rf.ask - 1.1 * 9773.0 * 0.002).abs() < 1e-6, "counterfactual ask {}", rf.ask); // 21.5
}

/// The law's seat: the same warm scheduler, the law on. n is the formula
/// (21.5 -> 22), not the shipped pooled value, and `[SUMCAP]` is not recorded.
#[test]
fn oracle43_a_the_law_owns_the_cap_when_on() {
    let tg = TestGauge::install();
    let mut sched = warm_one_path();
    let (shipped, _, _) = refresh(OracleGates::default(), &mut sched);
    assert!(shipped > 22, "the shipped law must differ from the oracle value here: {shipped}");
    let (cap, line, _) = refresh(gates(true, false, &[9773.0], None), &mut sched);
    assert_eq!(cap, 22, "n = ceil(1.1 · 9773 · 2 ms)");
    assert!(line.contains(" eng=0/0 "), "the oracle seat must not touch [SUMCAP]: {line}");
    let rf = tg.0.drain_refresh();
    assert_eq!((rf.n, rf.refreshes, rf.floor), (22, 1, 0));
    // The hold raises the lower bound through the same seat.
    let (cap, _, _) =
        refresh(gates(true, false, &[9773.0], Some((1536, 1.0e6))), &mut sched);
    assert_eq!(cap, 1536);
}
