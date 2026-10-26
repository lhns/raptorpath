//! Threading R1 (a): the receiver returns each drained inbound `Vec` to its
//! path's owner, and the owner reuses it (MEASUREMENT DISCIPLINE rule 1:
//! the mechanism under test must be shown to execute, through the SHIPPED
//! binary on both perf ends).
//!
//! The server's `[IOWN] phase=run` line carries `ibp_hit=` / `ibp_miss=`
//! (the pool's reuse count and its allocations). On a transfer of ~1700
//! source symbols the owner forwards hundreds of batches; the absolute
//! invariants are that almost every forward reused a returned `Vec`
//! (`hit >= 90 %` of the forwards) and that the allocations are bounded by
//! the pool's capacity, not by the traffic (`miss <= 2 * INBOUND_POOL_MAX`).
//!
//! Red on main ce60079: the line has no `ibp_hit=` token.

#[path = "common/loopback.rs"]
mod loopback;

/// `transport::io_owner::INBOUND_POOL_MAX` (a literal here so this file also
/// compiles, and fails by value, on the tree before R1).
const INBOUND_POOL_MAX: u64 = 64;

fn tok<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    line.split_whitespace()
        .find_map(|t| t.strip_prefix(key).and_then(|r| r.strip_prefix('=')))
}

fn num(line: &str, key: &str) -> u64 {
    tok(line, key)
        .unwrap_or_else(|| panic!("no `{key}=` token in {line}"))
        .parse()
        .unwrap()
}

fn server_log(n: usize) -> String {
    let env = [("RUST_LOG", "raptorpath=info"), ("RWM_RTOBS", "1")];
    let srv = loopback::spawn_perf_server(
        &loopback::free_addrs(n),
        &env,
        &["--protocol-hint", "bulk", "--window-reliable"],
    );
    let client = loopback::run_perf_client(&srv.addrs, &env, &loopback::perf_args("bulk", "2000000", "1"));
    assert!(client.contains("\"summary\""), "N={n}: the transfer did not complete:\n{client}");
    if cfg!(unix) {
        srv.stop_with("TERM", "[DIAG]")
    } else {
        srv.log_after(0, "\"server\":true")
    }
}

#[test]
fn the_owner_reuses_the_inbound_vecs_the_receiver_returns() {
    for n in [1usize, 2] {
        let log = server_log(n);
        let iown: Vec<&str> = log.lines().filter(|l| l.contains("[IOWN] phase=run")).collect();
        assert_eq!(iown.len(), n, "N={n}: one [IOWN] run line per owner:\n{log}");
        let (mut rx_batches, mut hit, mut miss) = (0, 0, 0);
        for l in &iown {
            rx_batches += num(l, "rx_batches");
            hit += num(l, "ibp_hit");
            miss += num(l, "ibp_miss");
        }
        assert!(rx_batches >= 100, "N={n}: too few forwards to judge ({rx_batches}):\n{log}");
        assert!(
            hit * 10 >= rx_batches * 9,
            "N={n}: the pool served {hit} of {rx_batches} forwards (miss {miss})"
        );
        assert!(
            miss <= 2 * INBOUND_POOL_MAX * n as u64,
            "N={n}: {miss} allocations: the pool is not bounding them"
        );
    }
}
