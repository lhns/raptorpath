//! Emission batching ships ON (status §8 Result (re-run), `FLIP-RECOMMENDED`;
//! Law 0, the burst bound `RWM_EMIT_BURST` = 64 at every path count). Both
//! positions of the knob, end to end through the shipped binary (`perf`
//! server + client over one loopback bind, bulk, `RWM_DIAG=1`):
//!
//!   * DEFAULT (no `RWM_*` in the endpoints' env; `loopback` clears every
//!     inherited one): `[GATES] RWM_EMIT_BATCH=1 RWM_EMIT_BURST=64`, the
//!     `emission batching ACTIVE` liveness echo, `eb_bursts > 0` on the
//!     client sender's last `[DIAG]` line, and no `OFF` echo;
//!   * `RWM_EMIT_BATCH=0` (the per-symbol control arm): `[GATES]
//!     RWM_EMIT_BATCH=0`, the `emission batching OFF` echo (rule 15c: with the
//!     default ON, the `=0` arm must witness that the knob reached the
//!     binary), no `ACTIVE` echo, and `eb_bursts=0`.
//!
//! Red on the tree before the flip: the default arm reads
//! `RWM_EMIT_BATCH=0` and no burst; the `OFF` echo does not exist.

#[path = "common/gauge.rs"]
mod gauge;
#[path = "common/loopback.rs"]
mod loopback;

const ARGS: [&str; 6] = ["--bytes", "8000000", "--runs", "1", "--protocol-hint", "bulk"];

/// Run one transfer with `env`; return (client log, client's last `[DIAG]`).
fn run(env: &[(&str, &str)]) -> (String, String) {
    let binds = loopback::free_addrs(1);
    let srv = loopback::spawn_perf_server(&binds, env, &["--protocol-hint", "bulk"]);
    let log = loopback::run_perf_client(&srv.addrs, env, &ARGS);
    let last = log
        .lines()
        .filter(|l| l.contains("[DIAG] "))
        .last()
        .unwrap_or_else(|| panic!("no [DIAG] line with RWM_DIAG=1:\n{log}"))
        .to_string();
    (log, last)
}

#[test]
fn emission_batching_is_on_by_default() {
    let (log, line) = run(&[("RWM_DIAG", "1"), ("RUST_LOG", "raptorpath=info")]);
    assert!(
        log.contains("RWM_EMIT_BATCH=1 RWM_EMIT_BURST=64"),
        "[GATES] must name batching ON at the measured burst 64:\n{log}"
    );
    assert!(!log.contains("RWM_EMIT_BATCH=0"), "default env carries the =0 side:\n{log}");
    assert!(log.contains("emission batching ACTIVE"), "the liveness echo is absent:\n{log}");
    assert!(!log.contains("emission batching OFF"), "default env printed the OFF echo:\n{log}");
    let bursts = gauge::u64_field(&line, "eb_bursts=");
    let syms = gauge::u64_field(&line, "eb_syms=");
    println!("default: eb_bursts={bursts} eb_syms={syms} [{}]", gauge::str_field(&line, "eb_end="));
    assert!(bursts > 0, "default env: no burst at all: {line}");
    assert!(syms <= 64 * bursts, "a burst exceeded the bound 64: {line}");
}

#[test]
fn the_off_position_echoes_itself() {
    let (log, line) = run(&[
        ("RWM_EMIT_BATCH", "0"),
        ("RWM_DIAG", "1"),
        ("RUST_LOG", "raptorpath=info"),
    ]);
    assert!(log.contains("RWM_EMIT_BATCH=0 RWM_EMIT_BURST=64"), "[GATES] lacks =0:\n{log}");
    assert!(!log.contains("RWM_EMIT_BATCH=1"), "the =0 arm carries the =1 side:\n{log}");
    assert!(
        log.contains("emission batching OFF (RWM_EMIT_BATCH=0"),
        "the =0 arm must echo that the knob reached the binary:\n{log}"
    );
    assert!(!log.contains("emission batching ACTIVE"), "=0 arm printed ACTIVE:\n{log}");
    assert!(!log.contains("emission batching out of scope"), "=0 read as scope-excluded:\n{log}");
    assert_eq!(gauge::u64_field(&line, "eb_bursts="), 0, "=0 arm burst: {line}");
}
