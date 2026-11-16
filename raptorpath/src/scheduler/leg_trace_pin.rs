//! W3.2 Phase A0 (docs/derivations/w3-per-leg-state.md §4): the per-leg
//! estimator trace pin. A fixed script through a `Scheduler` on a `MockClock`,
//! fed the way the three control handlers feed it, records at every step the
//! readings the Phase A relocation must not move. The table below was captured
//! on main before any A step. Every later step leaves it green, except the
//! exact columns the spec lets move (A1: `est` within the rounding bound, and
//! the post-revival `est`; each pinned in `leg_trace_a1_*` below).
//!
//! Columns, in table order:
//!   srtt  `PathState::srtt()` ns          est   `estimator.rtt()` ns (R2)
//!   meas  `srtt_measured()` ns or `-`     cad   Copa's cadence srtt ns
//!   min   `min_rtt()` ns or `-`           btlbw `btlbw_sym_per_s()` or `-`
//!   bdp   `copa_bdp_anchor()` or `-`      sig   `rtt_sigma_us()` or `-`
//!   jit   `rtt_jitter_us()`               wire  `wire_rtt()` ns or `-`
//!   cwnd  `PathState::cwnd`
//! f64 columns print with `{:?}` (shortest round-trip, i.e. exact).
//!
//! Re-capture only for a DELIBERATE model change:
//! `LEG_TRACE_PIN_PRINT=1 cargo test --lib leg_state_trace_pin -- --nocapture`.

use super::*;

const MS: fn(u64) -> Duration = Duration::from_millis;
const US: fn(u64) -> Duration = Duration::from_micros;

fn od<T: std::fmt::Display>(x: Option<T>) -> String {
    x.map_or("-".to_string(), |v| v.to_string())
}
fn of(x: Option<f64>) -> String {
    x.map_or("-".to_string(), |v| format!("{v:?}"))
}
fn ns(d: Duration) -> u128 {
    d.as_nanos()
}

// The three accessors Phase A steps re-point (access paths only; never the
// expected values): R2 as the recovery clocks read it, Copa's cadence srtt, and
// the handlers' echo feed.
fn est_of(p: &PathState) -> Duration {
    p.estimator.rtt()
}
fn cad_of(p: &PathState) -> Duration {
    p.copa.srtt.unwrap_or(DEFAULT_SRTT)
}
fn feed(p: &mut PathState, rtt: Duration) {
    p.estimator.record_rtt(rtt);
    p.record_rtt_sample(rtt);
}

/// One table row.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Row {
    pub label: String,
    pub srtt: u128,
    pub est: u128,
    pub meas: Option<u128>,
    pub cad: u128,
    pub min: Option<u128>,
    pub btlbw: Option<f64>,
    pub bdp: Option<f64>,
    pub sig: Option<u64>,
    pub jit: u64,
    pub wire: Option<u128>,
    pub cwnd: u32,
}

impl Row {
    fn line(&self) -> String {
        format!(
            "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
            self.label,
            self.srtt,
            self.est,
            od(self.meas),
            self.cad,
            od(self.min),
            of(self.btlbw),
            of(self.bdp),
            od(self.sig),
            self.jit,
            od(self.wire),
            self.cwnd
        )
    }
}

fn row(label: &str, p: &PathState) -> Row {
    Row {
        label: label.to_string(),
        srtt: ns(p.srtt()),
        est: ns(est_of(p)),
        meas: p.srtt_measured().map(ns),
        cad: ns(cad_of(p)),
        min: p.min_rtt().map(ns),
        btlbw: p.btlbw_sym_per_s(),
        bdp: p.copa_bdp_anchor(),
        sig: p.rtt_sigma_us(),
        jit: p.rtt_jitter_us(),
        wire: p.wire_rtt().map(ns),
        cwnd: p.cwnd,
    }
}

/// Feed one echo sample the way a control handler does.
fn echo(sched: &mut Scheduler, rtt: Duration) {
    feed(sched.path_mut(0).unwrap(), rtt);
}

/// The script. Returns the rows; also the data the witnesses read.
pub(super) fn run_script() -> Vec<Row> {
    assert!(
        crate::gates::get().mstar_anchor && !crate::gates::get().copa_wire,
        "leg_state_trace_pin is captured under default gates (MSTAR_ANCHOR on, COPA_WIRE off)"
    );
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new(clock.clone());
    sched.add_path(0);
    let mut rows = Vec::new();
    macro_rules! rec {
        ($l:expr) => {
            rows.push(row(&$l, sched.path(0).unwrap()))
        };
    }
    rec!("cold".to_string());

    // Phase 1 -- warm-up: 24 odd-µs echo samples, one delivery batch each,
    // all >= 5 ms apart, so the 8-sample anchor gate opens mid-phase.
    sched.path_mut(0).unwrap().set_wire_rtt(US(23_456));
    for i in 0..24u64 {
        clock.advance(MS(5 + i % 3));
        sched.touch_path(0);
        sched.path_mut(0).unwrap().charge_in_flight(14);
        echo(&mut sched, US(20_000 + 137 * (i * (i % 5 + 1)) + 3 * i + 1));
        sched.ack(0, 12);
        rec!(format!("w{i:02}"));
        if i == 0 {
            rec!("first-echo".to_string());
        }
        if i == 7 {
            rec!("8th-delivery".to_string());
        }
        if i == 11 {
            sched.path_mut(0).unwrap().set_wire_rtt(US(24_001));
            rec!("mid-warmup".to_string());
        }
    }
    rec!("warm".to_string());

    // Phase 2 -- a spike and the recovery from it.
    clock.advance(MS(6));
    echo(&mut sched, US(181_333));
    sched.ack(0, 9);
    rec!("spike".to_string());
    for i in 0..6u64 {
        clock.advance(MS(7));
        echo(&mut sched, US(19_777 + 11 * i));
        sched.ack(0, 10);
        rec!(format!("r{i}"));
    }
    rec!("recovered".to_string());

    // Phase 3 -- a 12 s idle gap (no echo, no delivery): everything holds.
    clock.advance(Duration::from_secs(12));
    rec!("idle-12s".to_string());
    // A delivery before any echo: evicts the RTT deque but leaves the cached
    // min_rtt (the R4 quirk, design §1.2).
    sched.ack(0, 8);
    rec!("idle-then-delivery".to_string());
    clock.advance(MS(9));
    echo(&mut sched, US(31_117));
    sched.ack(0, 8);
    rec!("idle-then-echo".to_string());
    sched.path_mut(0).unwrap().set_wire_rtt(US(26_500));

    // Phase 4 -- a dead-path timeout, then revival.
    sched.touch_path(0);
    clock.advance(Duration::from_secs(8));
    let dead = sched.check_dead_paths(Duration::from_secs(6));
    assert_eq!(dead, vec![0], "the path must time out");
    rec!("dead".to_string());
    sched.touch_path(0);
    assert!(sched.path(0).unwrap().active, "touch_path revives");
    rec!("revived-window".to_string());
    // A delivery in the window, before any post-revival echo.
    clock.advance(MS(8));
    sched.path_mut(0).unwrap().charge_in_flight(10);
    sched.ack(0, 10);
    rec!("revived-delivery".to_string());
    for i in 0..10u64 {
        clock.advance(MS(6 + i % 2));
        echo(&mut sched, US(41_007 + 211 * i));
        sched.ack(0, 11);
        rec!(format!("v{i}"));
        if i == 0 {
            rec!("revived-1st-echo".to_string());
        }
        if i == 3 {
            rec!("revived-4th-echo".to_string());
        }
    }
    rec!("revived-10th-echo".to_string());
    rows
}

/// The table captured at main (pre-A1), one line per row.
const GOLDEN: &[&str] = &[
    "cold|50000000|50000000|-|50000000|-|-|-|-|0|-|10",
    "w00|20001000|20001000|20001000|20001000|20001000|-|-|-|0|23456000|10",
    "first-echo|20001000|20001000|20001000|20001000|20001000|-|-|-|0|23456000|10",
    "w01|20035625|20035625|20035625|20035625|20001000|-|-|121|34|23456000|10",
    "w02|20134797|20134797|20134797|20134797|20001000|-|-|362|99|23456000|10",
    "w03|20324697|20324697|20324697|20324697|20001000|-|-|735|189|23456000|16",
    "w04|20628235|20628235|20628235|20628235|20001000|-|-|1238|303|23456000|16",
    "w05|20637331|20637331|20637331|20637331|20001000|-|-|1073|522|23456000|16",
    "w06|20765540|20765540|20765540|20765540|20001000|-|-|1031|577|23456000|16",
    "w07|21032222|21032222|21032222|21032222|20001000|2400.0|48.0024|1292|659|23456000|41",
    "8th-delivery|21032222|21032222|21032222|21032222|20001000|2400.0|48.0024|1292|659|23456000|41",
    "w08|21454319|21454319|21454319|21454319|20001000|2400.0|48.0024|1853|765|23456000|41",
    "w09|22046654|22046654|22046654|22046654|20001000|2400.0|48.0024|2621|893|23456000|41",
    "w10|21965947|21965947|21965947|21965947|20001000|2400.0|48.0024|2288|1380|23456000|41",
    "w11|22101204|22101204|22101204|22101204|20001000|2400.0|48.0024|2037|1413|23456000|63",
    "mid-warmup|22101204|22101204|22101204|22101204|20001000|2400.0|48.0024|2037|1413|24001000|63",
    "w12|22459679|22459679|22459679|22459679|20001000|2400.0|48.0024|2164|1477|24001000|63",
    "w13|23047719|23047719|23047719|23047719|20001000|2400.0|48.0024|2784|1566|24001000|63",
    "w14|23870879|23870879|23870879|23870879|20001000|2400.0|48.0024|3756|1679|24001000|63",
    "w15|23649644|23649644|23649644|23649644|20001000|2400.0|48.0024|3344|2411|24001000|63",
    "w16|23747564|23747564|23747564|23747564|20001000|2400.0|48.0024|2916|2401|24001000|96",
    "w17|24158993|24158993|24158993|24158993|20001000|2400.0|48.0024|2907|2426|24001000|96",
    "w18|24878994|24878994|24878994|24878994|20001000|2400.0|48.0024|3562|2483|24001000|96",
    "w19|25903245|25903245|25903245|25903245|20001000|2400.0|48.0024|4729|2567|24001000|96",
    "w20|25515464|25515464|25515464|25515464|20001000|2400.0|48.0024|4314|3530|24001000|96",
    "w21|25553281|25553281|25553281|25553281|20001000|2400.0|48.0024|3739|3466|24001000|145",
    "w22|25997746|25997746|25997746|25997746|20001000|2400.0|48.0024|3592|3444|24001000|145",
    "w23|26832278|26832278|26832278|26832278|20001000|2400.0|48.0024|4267|3459|24001000|145",
    "warm|26832278|26832278|26832278|26832278|20001000|2400.0|48.0024|4267|3459|24001000|145",
    "spike|46144868|46144868|46144868|46144868|20001000|2400.0|48.0024|67695|21609|24001000|145",
    "r0|42848884|42848884|42848884|42848884|19777000|2400.0|47.4648|59749|39102|24001000|145",
    "r1|39966273|39966273|39966273|39966273|19777000|2400.0|47.4648|52719|34216|24001000|145",
    "r2|37445364|37445364|37445364|37445364|19777000|2400.0|47.4648|46500|29940|24001000|219",
    "r3|35240944|35240944|35240944|35240944|19777000|2400.0|47.4648|41003|26199|24001000|219",
    "r4|33313451|33313451|33313451|33313451|19777000|2400.0|47.4648|36145|22925|24001000|219",
    "r5|31628270|31628270|31628270|31628270|19777000|2400.0|47.4648|31853|20061|24001000|219",
    "recovered|31628270|31628270|31628270|31628270|19777000|2400.0|47.4648|31853|20061|24001000|219",
    "idle-12s|31628270|31628270|31628270|31628270|19777000|2400.0|47.4648|31853|20061|24001000|219",
    "idle-then-delivery|31628270|31628270|31628270|31628270|19777000|-|-|31853|20061|24001000|330",
    "idle-then-echo|31564361|31564361|31564361|31564361|31117000|-|-|27586|18964|24001000|330",
    "dead|31564361|31564361|31564361|31564361|31117000|-|-|27586|18964|26500000|330",
    "revived-window|31564361|31564361|31564361|50000000|-|-|-|-|0|26500000|10",
    "revived-delivery|31564361|31564361|31564361|50000000|-|-|-|-|0|26500000|10",
    "v0|41007000|32744691|41007000|41007000|41007000|-|-|-|0|26500000|10",
    "revived-1st-echo|41007000|32744691|41007000|41007000|41007000|-|-|-|0|26500000|10",
    "v1|41033375|33803855|41033375|41033375|41007000|-|-|92|26|26500000|10",
    "v2|41082828|34756998|41082828|41082828|41007000|-|-|190|49|26500000|10",
    "v3|41152474|35617373|41152474|41152474|41007000|-|-|294|69|26500000|10",
    "revived-4th-echo|41152474|35617373|41152474|41152474|41007000|-|-|294|69|26500000|10",
    "v4|41239790|36396576|41239790|41239790|41007000|-|-|398|87|26500000|10",
    "v5|41342566|37104754|41342566|41342566|41007000|-|-|498|102|26500000|16",
    "v6|41458870|37750785|41458870|41458870|41007000|1833.3333333333333|75.1795|593|116|26500000|64",
    "v7|41587011|38342437|41587011|41587011|41007000|1833.3333333333333|75.1795|681|128|26500000|64",
    "v8|41725510|38886507|41725510|41725510|41007000|1833.3333333333333|75.1795|764|138|26500000|64",
    "v9|41873071|39388944|41873071|41873071|41007000|1833.3333333333333|75.1795|839|147|26500000|64",
    "revived-10th-echo|41873071|39388944|41873071|41873071|41007000|1833.3333333333333|75.1795|839|147|26500000|64",
];

#[test]
fn leg_state_trace_pin() {
    let rows = run_script();
    let got: Vec<String> = rows.iter().map(Row::line).collect();
    if std::env::var("LEG_TRACE_PIN_PRINT").is_ok() {
        for l in &got {
            println!("    \"{l}\",");
        }
    }
    assert_eq!(got.len(), GOLDEN.len(), "row count");
    for (g, w) in got.iter().zip(GOLDEN) {
        assert_eq!(g, w, "trace row differs (label|srtt|est|meas|cad|min|btlbw|bdp|sig|jit|wire|cwnd)");
    }
}

/// The witnesses: the script must exercise what the pin exists to pin, or it
/// passes vacuously.
#[test]
fn leg_state_trace_pin_witnesses() {
    let rows = run_script();
    let by = |l: &str| rows.iter().find(|r| r.label == l).unwrap().clone();
    // The anchor opens (>= 8 deliveries, an RTprop sample).
    assert!(by("8th-delivery").bdp.is_some() || by("mid-warmup").bdp.is_some());
    assert!(by("warm").btlbw.is_some() && by("warm").bdp.is_some());
    assert!(by("cold").bdp.is_none() && by("cold").min.is_none());
    // The echo samples are odd-µs: some row has nonzero sub-µs structure.
    assert!(rows.iter().any(|r| r.srtt % 1000 != 0));
    // The cached-min quirk: after the gap a delivery leaves min_rtt cached,
    // the next echo moves it.
    assert_eq!(by("idle-12s").min, by("recovered").min);
    assert_eq!(by("idle-then-delivery").min, by("idle-12s").min);
    assert_ne!(by("idle-then-echo").min, by("idle-then-delivery").min);
    // The idle gap holds every estimator.
    let (a, b) = (by("recovered"), by("idle-12s"));
    assert_eq!((a.srtt, a.est, a.min, a.sig, a.jit), (b.srtt, b.est, b.min, b.sig, b.jit));
    // Revival window: Copa is cleared (cadence at the 50 ms default, no RTprop,
    // no anchor, sigma gone), the pre-death srtt is far from 50 ms and is still
    // what srtt()/srtt_measured()/est.rtt() read, and the wire RTT survives.
    let w = by("revived-window");
    assert_eq!(w.cad, ns(DEFAULT_SRTT));
    assert!(w.min.is_none() && w.bdp.is_none() && w.btlbw.is_none() && w.sig.is_none());
    let pre = by("dead");
    assert_eq!(w.srtt, pre.srtt, "revival window keeps the stale srtt()");
    assert_eq!(w.meas, pre.meas);
    assert_eq!(w.est, pre.est);
    assert!(w.srtt.abs_diff(ns(DEFAULT_SRTT)) > 5_000_000, "stale srtt far from 50 ms");
    assert_eq!(w.wire, pre.wire, "wire_rtt survives revival");
    assert_eq!(w.cwnd, PathState::INITIAL_CWND);
    // The first post-revival echo reseeds Copa's srtt from the sample.
    assert_eq!(by("revived-1st-echo").cad, 41_007_000);
}

/// A0's second item: the leak-backstop bind gauge fires exactly when I > cwnd
/// and records the I/cwnd at the fire.
#[test]
fn leak_backstop_gauge_counts_fires_and_ratio() {
    let clock = Arc::new(MockClock::new());
    let mut p = PathState::new(0, clock);
    p.cwnd = 20;
    p.in_flight = 20;
    p.leak_backstop();
    assert_eq!((p.in_flight, p.leak_witness()), (20, (0, 0.0, 0.0)), "I == cwnd: no fire");
    p.in_flight = 40;
    p.leak_backstop();
    assert_eq!(p.in_flight, 30, "25 % decay unchanged");
    p.in_flight = 21;
    p.leak_backstop();
    assert_eq!(p.in_flight, 16);
    assert_eq!(p.leak_witness(), (2, 2.0 + 1.05, 2.0));
}
