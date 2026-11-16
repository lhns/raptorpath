//! The store law's gauge-only build (`net::slaw`; store-law.md R4.8/R5.6):
//! it prints and never applies. The cap sequence is bit-identical with the
//! gauge on and off, and with `RWM_DIAG` off none of its state exists.

use super::*;
use crate::gates::RuntimeGates;
use crate::net::sender_phases::StoreCapState;
use crate::net::sender_policy::SenderPolicy;
use crate::net::slaw::CapLog;

fn policy_with(diag: bool, slaw: bool) -> SenderPolicy {
    let mut g = RuntimeGates::resolve();
    g.store_paths = true;
    g.sum_cap = true;
    g.delta_cap = true;
    g.three_term = false;
    g.copa_feed = false;
    g.store_override = None;
    g.store_env_set = false;
    g.store_gain = 2.0;
    g.diag = diag;
    g.slaw = slaw;
    SenderPolicy::resolve(&g, 1200, ProtocolHint::Bulk, true, false, false, false)
}

fn policy(diag: bool) -> SenderPolicy {
    policy_with(diag, true)
}

/// What one driven run produced.
struct Run {
    caps: Vec<usize>,
    /// Echo samples fed / seen by the closed windows (gauge arm only).
    fed: u64,
    seen: u64,
    closes: u64,
    caplog_refreshes: u64,
    caplog_lines: Vec<String>,
    attached: bool,
    had_caplog: bool,
}

/// The recorded input trace: two legs, 60 refresh steps of 50 events each,
/// from a fixed LCG so both arms replay literally the same events. Per event
/// and leg: an echo sample, a send, a delivery 72 behind it, and a clock
/// advance whose size changes every 15 steps so the delivered rate (hence the
/// Copa anchor, hence the cap) moves through the run.
fn drive(gauge_on: bool) -> Run {
    let pol = policy(gauge_on);
    let clock = Arc::new(crate::scheduler::MockClock::new());
    let mut sched = Scheduler::new(clock.clone());
    if gauge_on {
        // As `diag::report` does on its first pass, before the paths exist
        // (so `add_path`'s attach is the path under test).
        sched.enable_slaw();
    }
    sched.add_path(0);
    sched.add_path(1);
    let attached = sched.path(0).unwrap().slaw_attached() && sched.path(1).unwrap().slaw_attached();
    let gates = RuntimeGates::resolve();
    let mut scs = StoreCapState::new(&pol);
    let had_caplog = scs.caplog.is_some();
    if let Some(c) = scs.caplog.as_mut() {
        c.sink = Some(Vec::new());
    }
    let mut sumcap =
        SumCapGauge::new(pol.sum_cap, pol.store_cap_floor, pol.store_path_pool, pol.delta_cap, pol.delta_b);
    let mut dcap =
        DeltaCapGauge::new(pol.delta_cap, pol.store_cap_floor, pol.store_path_pool, pol.sum_cap, pol.delta_b);
    let copa_feed: Option<Arc<CopaFeed>> = None;
    let mut lcg: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = move || {
        lcg = lcg.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (lcg >> 33) as u32
    };
    let (mut seq, mut fed, mut seen, mut closes) = ([0u64; 2], 0u64, 0u64, 0u64);
    let mut caps = Vec::new();
    for step in 0..60u64 {
        let dt = [41u64, 60, 25, 90][(step / 15) as usize % 4];
        for _ in 0..50 {
            for leg in 0..2usize {
                let rtt_us = 2000 * (leg as u32 + 1) + next() % 400;
                let p = sched.path_mut(leg as u32).unwrap();
                p.record_rtt_sample(Duration::from_micros(rtt_us as u64));
                fed += 1;
                p.on_src_sent(seq[leg], false);
                if seq[leg] >= 72 {
                    p.on_src_delivered_seq(seq[leg] - 72);
                }
                seq[leg] += 1;
            }
            clock.advance(Duration::from_micros(dt));
        }
        scs.dyn_cap_refresh_us = 0; // the seat throttles on the wall clock
        crate::net::sender_phases::refresh_store_cap(
            &mut scs,
            crate::net::sender_phases::StoreCapCtx {
                pol: &pol,
                gates: &gates,
                scheduler: &mut sched,
                copa_feed: &copa_feed,
                sumcap: &mut sumcap,
                dcap: &mut dcap,
            },
        );
        caps.push(scs.dyn_store_cap);
        if gauge_on && step % 5 == 4 {
            // A window close, as the 250 ms tick does it.
            for leg in 0..2u32 {
                if let Some(c) = sched.path_mut(leg).unwrap().slaw_close() {
                    seen += c.nk;
                    closes += 1;
                }
            }
        }
    }
    let cl: Option<&CapLog> = scs.caplog.as_deref();
    Run {
        caps,
        fed,
        seen,
        closes,
        caplog_refreshes: cl.map_or(0, |c| c.refreshes),
        caplog_lines: cl.and_then(|c| c.sink.clone()).unwrap_or_default(),
        attached,
        had_caplog,
    }
}

/// store-law.md R5.6: "drive the store-cap seat from a recorded input trace
/// with the gauge on and off and assert equal cap sequences" — and (rule 1)
/// prove the gauge arm executed, else equality proves nothing.
#[test]
fn cap_sequence_is_bit_identical_with_the_gauge_on_and_off() {
    let off = drive(false);
    let on = drive(true);
    assert_eq!(on.caps, off.caps, "the gauge changed the store cap");
    let distinct: std::collections::BTreeSet<_> = off.caps.iter().collect();
    assert!(distinct.len() > 4, "the trace must move the cap or equality is vacuous: {distinct:?}");

    // The gauge arm really ran.
    assert!(on.attached && on.had_caplog);
    assert_eq!(on.closes, 24, "2 legs x 12 closes");
    assert_eq!(on.seen, on.fed, "every echo sample fed reached a closed window");
    assert_eq!(on.caplog_refreshes, 60, "[CAPLOG] saw every refresh");
    assert!(!on.caplog_lines.is_empty());

    // The off arm constructed nothing and logged nothing.
    assert!(!off.attached && !off.had_caplog);
    assert_eq!((off.closes, off.seen, off.caplog_refreshes), (0, 0, 0));
    assert!(off.caplog_lines.is_empty());

    // Offline replay: every logged cap is reproduced by the pure law from
    // the PRINTED inputs (`{}` f64 formatting round-trips).
    let field = |l: &str, k: &str| -> String {
        l.split_whitespace().find_map(|t| t.strip_prefix(&format!("{k}="))).unwrap().to_string()
    };
    let mut pooled = 0;
    for l in &on.caplog_lines {
        if field(l, "br") != "pooled" {
            continue;
        }
        pooled += 1;
        let back = crate::net::pooled_store_cap(
            field(l, "on") == "1",
            field(l, "sum") == "1",
            field(l, "dc") == "1",
            field(l, "b").parse().unwrap(),
            field(l, "n").parse().unwrap(),
            field(l, "psum").parse().unwrap(),
            field(l, "gain").parse().unwrap(),
            field(l, "floor").parse().unwrap(),
            field(l, "pool").parse().unwrap(),
        );
        assert_eq!(back, Some(field(l, "cap").parse::<usize>().unwrap()), "{l}");
    }
    assert!(pooled > 4, "the seat must reach the pooled branch: {:?}", on.caplog_lines);
    // Cross-language witness: CAPLOG_DUMP=<file> writes the lines for
    // `docs/derivations/store-law/caplog_replay.py`.
    if let Ok(p) = std::env::var("CAPLOG_DUMP") {
        std::fs::write(p, on.caplog_lines.join("\n") + "\n").unwrap();
    }
}

/// With `RWM_DIAG` off no gauge state is constructed or fed: no sampler on a
/// path (new or via `add_path`), no `[CAPLOG]` recorder, no store-scope state,
/// no burst window; a sample reaches only Copa.
#[test]
fn with_diag_off_the_gauge_state_is_never_constructed() {
    let clock = Arc::new(crate::scheduler::MockClock::new());
    let mut sched = Scheduler::new(clock.clone());
    sched.add_path(0);
    let p = sched.path_mut(0).unwrap();
    assert!(!p.slaw_attached(), "a default path must not carry a sampler");
    p.record_rtt_sample(Duration::from_millis(2));
    assert!(!p.slaw_attached());
    assert!(p.slaw_close().is_none(), "nothing to close when nothing was attached");
    assert!(p.copa_min_rtt().is_some(), "the sample still reached Copa");

    let off = policy(false);
    assert!(!off.diag_on);
    assert!(StoreCapState::new(&off).caplog.is_none(), "no [CAPLOG] recorder with diag off");
    assert!(StoreCapState::new(&policy(true)).caplog.is_some(), "and one with diag on");

    // RWM_SLAW=0 under RWM_DIAG (the perturbation control): [DIAG] stays on,
    // the gauge's work does not run, and RWM_SLAW alone (diag off) does nothing.
    let ctl = policy_with(true, false);
    assert!(ctl.diag_on && !ctl.slaw_on, "diag on, gauge off");
    assert!(StoreCapState::new(&ctl).caplog.is_none(), "no [CAPLOG] recorder with RWM_SLAW=0");
    assert!(!policy_with(false, true).slaw_on, "RWM_SLAW is effective only under RWM_DIAG");
    assert!(policy_with(true, true).slaw_on);

    let dg = crate::net::diag::DiagState::new(0, 0, 0, 0);
    assert!(dg.slaw.is_none(), "the store-scope state is armed by diag::report only");
    let mut eb = crate::net::emit_burst::EmitBurstGauge::default();
    eb.begin();
    eb.on_symbol(0);
    eb.end(crate::net::emit_burst::BurstEnd::Drained);
    assert!(eb.take_window().is_empty(), "no burst window without enable_window");
    eb.enable_window();
    eb.begin();
    eb.on_symbol(0);
    eb.on_symbol(0);
    eb.end(crate::net::emit_burst::BurstEnd::Cap);
    assert_eq!(eb.take_window(), vec![2]);
    assert!(eb.take_window().is_empty(), "take drains");
}

/// C02's truncation counter: `sack_gaps_truncated` is true exactly when
/// `sack_to_gaps` stopped at `MAX_NACK_GAPS` with more gaps left, and
/// `sack_to_gaps`'s own output is the same either way.
#[test]
fn gap_truncation_flag_is_true_only_when_more_gaps_remained() {
    let ranges = |k: usize| -> Vec<(u64, u64)> { (0..k as u64).map(|i| (2 * i + 1, 2 * i + 1)).collect() };
    for k in [0usize, 1, MAX_NACK_GAPS - 1, MAX_NACK_GAPS] {
        assert!(!sack_gaps_truncated(0, &ranges(k)), "{k} gaps fit the cap");
        assert_eq!(sack_to_gaps(0, &ranges(k)).len(), k);
    }
    // k = MAX + 1 and beyond: the report keeps MAX and one or more remain.
    for k in [MAX_NACK_GAPS + 1, MAX_NACK_GAPS + 5, 5 * MAX_NACK_GAPS] {
        assert!(sack_gaps_truncated(0, &ranges(k)), "{k} gaps exceed the cap");
        assert_eq!(sack_to_gaps(0, &ranges(k)).len(), MAX_NACK_GAPS, "output unchanged");
    }
    // Ranges that leave no hole are not gaps.
    assert!(!sack_gaps_truncated(5, &[(5, 9), (10, 20)]));
    assert!(!sack_gaps_truncated(0, &[]));
}
