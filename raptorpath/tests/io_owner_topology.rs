//! Threading Q1: the per-path I/O owner, reached by the SHIPPED binary on
//! both perf ends (measurement-discipline rule 1; CLAUDE.md: assert the
//! wiring routes there). Clauses at N = 1, 2 and 8 paths:
//!
//!   1. one `[IOWN] phase=run` line per path on both ends — every path has
//!      its owner, and the owner's instrument ran (`polls > 0`), on the main
//!      runtime (`rt=main`);
//!   2. **the thread-role set is the same at every N** (Linux, from the
//!      client's `[THR] os phase=run` lines: thread names with the trailing
//!      `-<index>` removed) — the structure does not change with the path
//!      count, and no dedicated I/O runtime thread exists.
//!
//! Red on main 69fd846: no `[IOWN]` line. Q2 step 0 (the `own` placement
//! deleted, status §13): the `RWM_IO_RT` echo, the `[TOPO]` line and the
//! `own`-only driver-routing clause went with the arm they tested.

#[path = "common/loopback.rs"]
mod loopback;

use std::collections::BTreeSet;

fn tok<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    line.split_whitespace()
        .find_map(|t| t.strip_prefix(key).and_then(|r| r.strip_prefix('=')))
}

/// `rp-w-12` → `rp-w`, `raptorpath` → `raptorpath`.
fn role(comm: &str) -> String {
    match comm.rsplit_once('-') {
        Some((head, idx)) if !idx.is_empty() && idx.chars().all(|c| c.is_ascii_digit()) => {
            head.to_string()
        }
        _ => comm.to_string(),
    }
}

struct Run {
    client: String,
    server: String,
}

fn run(n: usize) -> Run {
    let env = [("RUST_LOG", "raptorpath=info"), ("RWM_RTOBS", "1")];
    let srv = loopback::spawn_perf_server(
        &loopback::free_addrs(n),
        &env,
        &["--protocol-hint", "bulk", "--window-reliable"],
    );
    let client = loopback::run_perf_client(&srv.addrs, &env, &loopback::perf_args("bulk", "2000000", "1"));
    assert!(client.contains("\"summary\""), "N={n}: the transfer did not complete:\n{client}");
    let server = if cfg!(unix) {
        srv.stop_with("TERM", "[DIAG]")
    } else {
        srv.log_after(0, "\"server\":true")
    };
    Run { client, server }
}

#[test]
fn the_io_owner_topology_is_the_same_at_every_path_count() {
    let mut roles: Vec<(usize, BTreeSet<String>)> = Vec::new();
    for n in [1usize, 2, 8] {
        let r = run(n);
        for (side, log) in [("client", &r.client), ("server", &r.server)] {
            let iown: Vec<&str> = log
                .lines()
                .filter(|l| l.contains("[IOWN] phase=run"))
                .collect();
            assert_eq!(iown.len(), n, "{side} N={n}: one [IOWN] run line per owner:\n{log}");
            for l in &iown {
                assert_eq!(tok(l, "rt"), Some("main"), "{side} N={n}: owner off the main runtime: {l}");
                let polls: u64 = tok(l, "polls").expect("polls").parse().unwrap();
                assert!(polls > 0, "{side} N={n}: the owner never polled: {l}");
            }
            assert!(!log.contains("[TOPO] io_rt="), "{side} N={n}: the placement echo is gone");
        }
        if cfg!(target_os = "linux") {
            let set: BTreeSet<String> = r
                .client
                .lines()
                .filter(|l| l.contains("[THR] os phase=run side=client"))
                .filter_map(|l| tok(l, "comm"))
                .map(role)
                .collect();
            assert!(set.contains("rp-w"), "N={n}: no runtime worker in {set:?}");
            assert!(!set.contains("rp-io"), "N={n}: a dedicated I/O runtime thread exists: {set:?}");
            roles.push((n, set));
        }
    }
    for w in roles.windows(2) {
        assert_eq!(
            w[0].1, w[1].1,
            "the thread-role set changed between N={} and N={}",
            w[0].0, w[1].0
        );
    }
}
