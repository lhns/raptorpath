//! ADR-0079 / S06: the taper copy branch is deleted. Every proactive slot
//! carries a coded symbol, so `[DIAG] taper=` and `plost=` read 0 at every
//! dial point while `pslot=` (slots taken) is positive.
//!
//! T1 (`no_taper_copy_with_slots_*`): red on main (taper > 0 at the six
//! points off Bulk), green on the deletion. The Bulk triple is NOT a witness:
//! Bulk takes no slots (rr = 0), so it asserts `taper == 0` only and proves
//! nothing about the branch.
//!
//! Preconditions, each with a witness read from the log, never assumed:
//! * the dial point reached both endpoints (`[GATES] RWM_DELTA=`);
//! * the loss reached the estimator (`pl=` > 0 on a path);
//! * slots were taken (`pslot=` > 0).
//!
//! Run with `--test-threads=1`: every test is a pair of processes on a shaped
//! link, and parallel runs skew each other's timing.

#[path = "common/gauge.rs"]
mod gauge;
#[path = "common/loopback.rs"]
mod loopback;

use gauge::{numeric_prefix, raw_field};

/// Seeded i.i.d. loss, ~2.4 %: `custom:<Mbit>;<ow_ms>;<jit_ms>;<ge_p %>;<ge_q %>`
/// with ge_q = 100 % every bad state lasts one packet. 40 Mbit bounds the wall
/// time below so the 250 ms `[DIAG]` tick fires many times.
const CELL: &str = "custom:40;10;0;2.5;100";

/// Max of the numeric `key` over every token of the log (the last `[DIAG]`
/// line can be cut by the SIGKILL, so a max over lines is read, as in
/// `holeclass_reachability`).
fn max_field(log: &str, key: &str) -> Option<f64> {
    log.split_whitespace()
        .filter_map(|t| t.strip_prefix(key))
        .filter_map(|v| numeric_prefix(v).parse::<f64>().ok())
        .fold(None, |m, v| Some(m.map_or(v, |m: f64| m.max(v))))
}

struct Reading {
    taper: u64,
    plost: u64,
    pslot: Option<u64>,
    eps: f64,
    cli: String,
}

/// One transfer at `hint`, optionally `RWM_DELTA=delta` on both ends,
/// `paths` paths each shaped by [`CELL`], on the reliable or evicting seat.
fn run(hint: &str, delta: Option<&str>, paths: usize, reliable: bool) -> Reading {
    let mut env: Vec<(&str, &str)> = vec![("RWM_DIAG", "1"), ("RUST_LOG", "raptorpath=info")];
    if let Some(d) = delta {
        env.push(("RWM_DELTA", d));
    }
    let netem = vec![CELL; paths].join(",");
    let client_env = [("RWM_L0_NETEM", netem.as_str()), ("RWM_L0_SEED", "42")];
    let (cli, srv) = loopback::transfer(loopback::Transfer {
        paths,
        env: &env,
        client_env: &client_env,
        hint,
        bytes: "12000000",
        runs: "1",
        reliable,
        ..Default::default()
    });
    // Wiring witness: the dial value reached both ends.
    if let Some(d) = delta {
        for (who, log) in [("client", &cli), ("server", &srv)] {
            let line = log
                .lines()
                .find(|l| l.contains("[GATES]"))
                .unwrap_or_else(|| panic!("{who}: no [GATES] line"));
            let got = raw_field(line, "RWM_DELTA=").unwrap_or("<absent>");
            assert!(
                numeric_prefix(got).parse::<f64>().ok() == d.parse::<f64>().ok(),
                "{who}: RWM_DELTA did not arrive (want {d}, [GATES] says {got}): {line}"
            );
        }
    }
    let taper = max_field(&cli, "taper=").unwrap_or_else(|| panic!("no taper= in:\n{cli}")) as u64;
    let plost = max_field(&cli, "plost=").unwrap_or_else(|| panic!("no plost= in:\n{cli}")) as u64;
    Reading {
        taper,
        plost,
        pslot: max_field(&cli, "pslot=").map(|v| v as u64),
        eps: max_field(&cli, " pl=").or_else(|| max_field(&cli, "pl=")).unwrap_or(0.0),
        cli,
    }
}

/// The six points off Bulk carry the test: slots exist and the estimator saw
/// loss, so main copies there and the deletion must not.
fn assert_no_copy_with_slots(tag: &str, r: &Reading) {
    println!(
        "[s06] {tag}: taper={} plost={} pslot={:?} eps_hat_max={:.4}",
        r.taper, r.plost, r.pslot, r.eps
    );
    let pslot = r.pslot.unwrap_or_else(|| panic!("{tag}: `pslot=` is not printed:\n{}", r.cli));
    assert!(pslot > 0, "{tag}: no proactive slot was taken (pslot=0): not a witness");
    assert!(r.eps > 0.0, "{tag}: the estimator saw no loss (pl=0): not a witness");
    assert_eq!(r.taper, 0, "{tag}: a proactive slot carried a source copy (taper={})", r.taper);
    assert_eq!(r.plost, 0, "{tag}: plost={} (the DIAG-gated copy count)", r.plost);
}

/// Bulk: no slots, so only `taper == 0` is asserted. Proves nothing about the
/// branch; it is here so the nine-point triple is run.
fn assert_bulk_point(tag: &str, r: &Reading) {
    println!("[s06] {tag}: taper={} plost={} pslot={:?} (Bulk: not a witness)", r.taper, r.plost, r.pslot);
    assert_eq!(r.taper, 0, "{tag}: taper={}", r.taper);
    assert_eq!(r.plost, 0, "{tag}: plost={}", r.plost);
}

macro_rules! point {
    ($name:ident, $hint:expr, $delta:expr, $bulk:expr) => {
        #[test]
        fn $name() {
            let r = run($hint, $delta, 1, true);
            if $bulk {
                assert_bulk_point(stringify!($name), &r);
            } else {
                assert_no_copy_with_slots(stringify!($name), &r);
            }
        }
    };
}

// Bulk triple: the named hint and the ±2 % nudges on the Auto carrier.
point!(no_taper_copy_bulk_named, "bulk", None, true);
point!(no_taper_copy_bulk_minus, "auto", Some("0.0049"), true);
point!(no_taper_copy_bulk_plus, "auto", Some("0.0051"), true);
// Auto triple.
point!(no_taper_copy_auto_named, "auto", None, false);
point!(no_taper_copy_auto_minus, "auto", Some("0.49"), false);
point!(no_taper_copy_auto_plus, "auto", Some("0.51"), false);
// Realtime triple.
point!(no_taper_copy_realtime_named, "realtime", None, false);
point!(no_taper_copy_realtime_minus, "auto", Some("49"), false);
point!(no_taper_copy_realtime_plus, "auto", Some("51"), false);

/// The seat and N variants, at Auto (0.5) only, to bound the runtime. N = 1
/// reliable is `no_taper_copy_auto_named`.
#[test]
fn no_taper_copy_auto_evicting_n1() {
    assert_no_copy_with_slots("auto_evicting_n1", &run("auto", Some("0.5"), 1, false));
}

#[test]
fn no_taper_copy_auto_reliable_n2() {
    assert_no_copy_with_slots("auto_reliable_n2", &run("auto", Some("0.5"), 2, true));
}

#[test]
fn no_taper_copy_auto_evicting_n2() {
    assert_no_copy_with_slots("auto_evicting_n2", &run("auto", Some("0.5"), 2, false));
}
