//! Threading Q1: the per-path I/O owner, reached by the SHIPPED binary on
//! both perf ends (measurement-discipline rule 1; CLAUDE.md: assert the
//! wiring routes there). Clauses, for each placement arm
//! (`RWM_IO_RT=shared|own`) at N = 1, 2 and 8 paths:
//!
//!   1. the gate is echoed on both ends' `[GATES]` (`RWM_IO_RT=<arm>`,
//!      two-sided across the two arms, rule 15c);
//!   2. one `[TOPO] io_rt=<arm>` line per path on both ends, naming the
//!      runtime the owner runs on: `main` under `shared`, `rp-io-<k>` under
//!      `own` with `k < K` and K = the `k=` the line prints;
//!   3. **the thread-role set is the same at every N** (Linux, from the
//!      client's `[THR] os phase=run` lines: thread names with the trailing
//!      `-<index>` removed) — the structure does not change with the path
//!      count; under `own` the `rp-io-<k>` names are exactly `0..K` at every
//!      N (the pool is created whole);
//!   4. **driver routing under `own`**: every owner's `[IOWN]` line reads
//!      `drv_off=0` with `drv_on > 0` — quinn's ConnectionDriver ran every
//!      transmit on the owner's thread (the routing probe wraps the
//!      congestion controller; `on_sent` is called inside the driver's
//!      poll). Under `shared` the same line is printed (the prediction is
//!      `drv_off > 0`; not asserted here — it is the battery's mechanism
//!      read).
//!
//! Red on main 69fd846: no `RWM_IO_RT` echo, no `[TOPO]`/`[IOWN]` line.

#[path = "common/loopback.rs"]
mod loopback;

use std::collections::BTreeSet;

fn tok<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    line.split_whitespace()
        .find_map(|t| t.strip_prefix(key).and_then(|r| r.strip_prefix('=')))
}

/// `rp-io-3` → `rp-io`, `rp-w-12` → `rp-w`, `raptorpath` → `raptorpath`.
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

fn run(arm: &str, n: usize) -> Run {
    let env = [
        ("RUST_LOG", "raptorpath=info"),
        ("RWM_RTOBS", "1"),
        ("RWM_IO_RT", arm),
    ];
    let srv = loopback::spawn_perf_server(
        &loopback::free_addrs(n),
        &env,
        &["--protocol-hint", "bulk", "--window-reliable"],
    );
    let client = loopback::run_perf_client(&srv.addrs, &env, &loopback::perf_args("bulk", "2000000", "1"));
    assert!(client.contains("\"summary\""), "{arm} N={n}: the transfer did not complete:\n{client}");
    let server = if cfg!(unix) {
        srv.stop_with("TERM", "[DIAG]")
    } else {
        srv.log_after(0, "\"server\":true")
    };
    Run { client, server }
}

fn topo_lines<'a>(log: &'a str) -> Vec<&'a str> {
    log.lines().filter(|l| l.contains("[TOPO] io_rt=")).collect()
}

fn check_topo(log: &str, side: &str, arm: &str, n: usize) -> Option<usize> {
    let t = topo_lines(log);
    assert_eq!(t.len(), n, "{side} {arm} N={n}: expected one [TOPO] per path:\n{}", t.join("\n"));
    let mut k_seen = None;
    for l in &t {
        assert_eq!(tok(l, "io_rt"), Some(arm), "{side}: wrong arm on {l}");
        let rt = tok(l, "rt").expect("rt=");
        match arm {
            "shared" => assert_eq!(rt, "main", "{side}: shared owner off the main runtime: {l}"),
            _ => {
                let k: usize = tok(l, "k").expect("k=").parse().expect("k num");
                let idx: usize = rt
                    .strip_prefix("rp-io-")
                    .unwrap_or_else(|| panic!("{side}: own owner not on rp-io-*: {l}"))
                    .parse()
                    .expect("rp-io index");
                assert!(idx < k, "{side}: runtime index {idx} ≥ K = {k}: {l}");
                k_seen = Some(k);
            }
        }
    }
    k_seen
}

#[test]
fn the_io_owner_topology_is_the_same_at_every_path_count() {
    for arm in ["shared", "own"] {
        let mut roles: Vec<(usize, BTreeSet<String>)> = Vec::new();
        for n in [1usize, 2, 8] {
            let r = run(arm, n);
            for (side, log) in [("client", &r.client), ("server", &r.server)] {
                assert!(
                    log.contains(&format!("RWM_IO_RT={arm}")),
                    "{side} {arm} N={n}: [GATES] does not echo RWM_IO_RT={arm}"
                );
                let other = if arm == "own" { "shared" } else { "own" };
                assert!(!log.contains(&format!("RWM_IO_RT={other}")), "{side}: both arms echoed");
                let k = check_topo(log, side, arm, n);
                let iown: Vec<&str> = log
                    .lines()
                    .filter(|l| l.contains("[IOWN] phase=run"))
                    .collect();
                assert_eq!(iown.len(), n, "{side} {arm} N={n}: one [IOWN] run line per owner:\n{log}");
                if arm == "own" {
                    for l in &iown {
                        let on: u64 = tok(l, "drv_on").expect("drv_on").parse().unwrap();
                        let off: u64 = tok(l, "drv_off").expect("drv_off").parse().unwrap();
                        assert!(on > 0, "{side} N={n}: the routing probe saw no driver transmit: {l}");
                        assert_eq!(off, 0, "{side} N={n}: quinn's driver polled OFF the owner thread: {l}");
                    }
                    let k = k.expect("own prints k=");
                    if cfg!(target_os = "linux") && side == "client" {
                        let io: BTreeSet<String> = log
                            .lines()
                            .filter(|l| l.contains("[THR] os phase=run side=client"))
                            .filter_map(|l| tok(l, "comm"))
                            .filter(|c| c.starts_with("rp-io-"))
                            .map(str::to_string)
                            .collect();
                        let want: BTreeSet<String> = (0..k).map(|i| format!("rp-io-{i}")).collect();
                        assert_eq!(io, want, "client own N={n}: the I/O thread set is not 0..K");
                    }
                }
            }
            if cfg!(target_os = "linux") {
                let set: BTreeSet<String> = r
                    .client
                    .lines()
                    .filter(|l| l.contains("[THR] os phase=run side=client"))
                    .filter_map(|l| tok(l, "comm"))
                    .map(role)
                    .collect();
                assert!(set.contains("rp-w"), "{arm} N={n}: no runtime worker in {set:?}");
                assert_eq!(
                    set.contains("rp-io"),
                    arm == "own",
                    "{arm} N={n}: the rp-io role must exist exactly under own: {set:?}"
                );
                roles.push((n, set));
            }
        }
        for w in roles.windows(2) {
            assert_eq!(
                w[0].1, w[1].1,
                "{arm}: the thread-role set changed between N={} and N={}",
                w[0].0, w[1].0
            );
        }
    }
}
