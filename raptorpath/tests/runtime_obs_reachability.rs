//! The threading-redesign P0 instrument (`raptorpath::runtime_obs`) is reached by
//! the SHIPPED binary on both perf ends (measurement-discipline rule 1: prove
//! the mechanism under test executes; CLAUDE.md: assert the wiring routes
//! there). A unit test of `runtime_obs` alone cannot show that `main` builds its
//! runtime through `runtime_obs::build_runtime`, arms the probe, and that the perf
//! client/server bracket their objects. Clauses:
//!
//!   1. client: one `[THR] rt phase=xfer … run=1` line per tokio worker, a
//!      `[THR] sum phase=xfer` line and a `[LAG] phase=xfer … run=1` line
//!      with n > 0 samples; the same set at `phase=run` (process end, the
//!      `[LAG]` line `final=1`).
//!   2. the worker count is the same in the xfer and run tables (one
//!      runtime), and every worker's `busy_frac` is in [0, 1.05].
//!   3. Linux: per-thread `[THR] os` lines exist, at least one carries a
//!      `comm=rp-w-` name (the builder's `thread_name_fn` executed) and one
//!      the main thread's `comm=raptorpath`; elsewhere `[THR] os unavailable`.
//!   4. server (stopped with SIGTERM, as `pkill -x raptorpath` does, unix
//!      only): `[THR] rt phase=xfer … obj=1` and the `phase=run` set.
//!   5. Opt-in (threading P2a step 0): all of the above with `RWM_RTOBS=1`,
//!      echoed `RWM_RTOBS=1` on both ends' `[GATES]`; without it, both ends
//!      echo `RWM_RTOBS=0` and print NO `[THR]` / `[LAG]` line at all (the
//!      probe task and the `/proc` reads do not exist) — two-sided, rule 15c.

#[path = "common/loopback.rs"]
mod loopback;

const ENV: [(&str, &str); 2] = [("RUST_LOG", "raptorpath=info"), ("RWM_RTOBS", "1")];
const ENV_OFF: [(&str, &str); 1] = [("RUST_LOG", "raptorpath=info")];

fn lines<'a>(log: &'a str, head: &str) -> Vec<&'a str> {
    log.lines().filter(|l| l.contains(head)).collect()
}

fn tok<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    line.split_whitespace().find_map(|t| t.strip_prefix(key).and_then(|r| r.strip_prefix('=')))
}

fn assert_side(log: &str, side: &str, window: &str) {
    let xfer_rt = lines(log, &format!("[THR] rt phase=xfer side={side} {window}"));
    let run_rt = lines(log, &format!("[THR] rt phase=run side={side} "));
    assert!(!xfer_rt.is_empty(), "{side}: no [THR] rt phase=xfer {window} line:\n{log}");
    assert!(!run_rt.is_empty(), "{side}: no [THR] rt phase=run line:\n{log}");
    assert_eq!(
        xfer_rt.len(),
        run_rt.len(),
        "{side}: the xfer and run tables name different worker counts (two runtimes?)"
    );
    for l in xfer_rt.iter().chain(run_rt.iter()) {
        let f: f64 = tok(l, "busy_frac").expect("busy_frac").parse().expect("busy_frac num");
        assert!((0.0..=1.05).contains(&f), "{side}: busy_frac out of range: {l}");
    }
    let sums = lines(log, &format!("[THR] sum phase=xfer side={side} {window}"));
    assert_eq!(sums.len(), 1, "{side}: expected one [THR] sum xfer line:\n{log}");
    assert_eq!(
        tok(sums[0], "workers").and_then(|w| w.parse::<usize>().ok()),
        Some(xfer_rt.len()),
        "{side}: sum line's workers= disagrees with the rt lines: {}",
        sums[0]
    );
    let lag = lines(log, &format!("[LAG] phase=xfer side={side} {window}"));
    assert_eq!(lag.len(), 1, "{side}: expected one [LAG] xfer line:\n{log}");
    let n: usize = tok(lag[0], "n").expect("n").parse().expect("n num");
    assert!(n > 0, "{side}: the lag probe sampled nothing in the window: {}", lag[0]);
    let lag_run = lines(log, &format!("[LAG] phase=run side={side} "));
    assert_eq!(lag_run.len(), 1, "{side}: expected one [LAG] run line:\n{log}");
    assert!(lag_run[0].contains("final=1"), "{}", lag_run[0]);
    if cfg!(target_os = "linux") {
        let os = lines(log, &format!("[THR] os phase=xfer side={side} {window}"));
        assert!(
            os.iter().any(|l| l.contains("comm=rp-w-")),
            "{side}: no worker thread named rp-w-* — thread_name_fn did not execute:\n{log}"
        );
        assert!(
            os.iter().any(|l| l.contains("comm=raptorpath")),
            "{side}: the main (block_on) thread is missing from [THR] os:\n{log}"
        );
    } else {
        assert!(log.contains("[THR] os unavailable"), "{side}: no os-unavailable line");
    }
}

#[test]
fn the_thr_and_lag_lines_fire_on_both_perf_ends() {
    let srv = loopback::spawn_perf_server(
        &[loopback::free_addr()],
        &ENV,
        &["--protocol-hint", "bulk", "--window-reliable"],
    );
    let log = loopback::run_perf_client(&srv.addrs, &ENV, &loopback::perf_args("bulk", "4000000", "1"));
    assert!(log.contains("\"summary\""), "the transfer did not complete:\n{log}");
    assert!(log.contains("RWM_RTOBS=1"), "client: [GATES] does not echo RWM_RTOBS=1:\n{log}");
    assert_side(&log, "client", "run=1");

    // The server's object window lines are printed at completion; its
    // phase=run lines ride the SIGTERM graceful path (unix only).
    let mark = 0;
    let srv_log_now = srv.log_after(mark, "[LAG] phase=xfer side=server obj=1");
    assert!(
        srv_log_now.contains("[THR] rt phase=xfer side=server obj=1"),
        "server: no [THR] rt phase=xfer obj=1 line:\n{srv_log_now}"
    );
    assert!(srv_log_now.contains("RWM_RTOBS=1"), "server: [GATES] does not echo RWM_RTOBS=1");
    if cfg!(unix) {
        let srv_log = srv.stop_with("TERM", "[DIAG]");
        assert_side(&srv_log, "server", "obj=1");
    }
}

/// The off side of the opt-in: the default (no `RWM_RTOBS`) prints no
/// `[THR]` and no `[LAG]` line on either end, and both echo `RWM_RTOBS=0`.
#[test]
fn without_rtobs_neither_end_prints_thr_or_lag() {
    let srv = loopback::spawn_perf_server(
        &[loopback::free_addr()],
        &ENV_OFF,
        &["--protocol-hint", "bulk", "--window-reliable"],
    );
    let log = loopback::run_perf_client(&srv.addrs, &ENV_OFF, &loopback::perf_args("bulk", "4000000", "1"));
    assert!(log.contains("\"summary\""), "the transfer did not complete:\n{log}");
    assert!(log.contains("RWM_RTOBS=0"), "client: [GATES] does not echo RWM_RTOBS=0:\n{log}");
    assert!(
        !log.contains("[THR]") && !log.contains("[LAG]"),
        "client: instrument lines without RWM_RTOBS:\n{log}"
    );
    let srv_log = if cfg!(unix) {
        srv.stop_with("TERM", "[DIAG]")
    } else {
        srv.log_after(0, "\"server\":true")
    };
    assert!(srv_log.contains("RWM_RTOBS=0"), "server: [GATES] does not echo RWM_RTOBS=0:\n{srv_log}");
    assert!(
        srv_log.contains("\"server\":true"),
        "server: the object never completed (the absence below would be vacuous):\n{srv_log}"
    );
    assert!(
        !srv_log.contains("[THR]") && !srv_log.contains("[LAG]"),
        "server: instrument lines without RWM_RTOBS:\n{srv_log}"
    );
}
