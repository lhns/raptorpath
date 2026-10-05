//! The endpoint receive-buffer request reaches the shipped binary's sockets
//! and says so (transport/rcvbuf.rs; measurement-discipline rule 1).
//! Clauses, in the order they can fail:
//!
//!   1. Both endpoints print one `[RCVBUF]` echo per path socket, with the
//!      role each binds as (`server` on the receiver, `client` on the
//!      sender), and every field the bind gauge needs.
//!   2. The echoed request is the formula's value (4 000 000 B), and on
//!      Linux the echoed grant reaches `2 × min(req, rmem_max)` (the kernel
//!      doubles; `2 × req` as root) and `clamped=` agrees with it.
//!   3. With `RWM_DIAG=1` the receiver's `[CTLD]` line carries the
//!      per-socket kernel drop counter `rxdrop<path>=` for every path on
//!      Linux (it reads `SO_MEMINFO`, absent elsewhere — never a zero
//!      substituted for "no counter").

#[path = "common/loopback.rs"]
mod loopback;

const REQ: usize = 4_000_000;

fn field<'a>(line: &'a str, key: &str) -> &'a str {
    line.split_whitespace()
        .find_map(|t| t.strip_prefix(key))
        .unwrap_or_else(|| panic!("`{key}` missing from: {line}"))
}

#[cfg(target_os = "linux")]
fn floor() -> usize {
    // SAFETY: geteuid has no preconditions.
    if unsafe { libc::geteuid() } == 0 {
        return 2 * REQ;
    }
    let rmem_max: usize = std::fs::read_to_string("/proc/sys/net/core/rmem_max")
        .expect("rmem_max readable")
        .trim()
        .parse()
        .expect("rmem_max parses");
    2 * REQ.min(rmem_max)
}

fn echoes(log: &str) -> Vec<&str> {
    log.lines().filter(|l| l.contains("[RCVBUF]")).collect()
}

#[test]
fn every_endpoint_socket_echoes_its_receive_buffer_grant() {
    let paths = 2;
    let (cli, srv) = loopback::transfer(loopback::Transfer {
        paths,
        env: &[("RWM_DIAG", "1")],
        ..loopback::Transfer::default()
    });

    for (log, role) in [(&srv, "server"), (&cli, "client")] {
        let lines = echoes(log);
        assert_eq!(lines.len(), paths, "one [RCVBUF] echo per {role} socket:\n{log}");
        for (i, l) in lines.iter().enumerate() {
            let l = &l[l.find("[RCVBUF]").unwrap()..];
            assert!(l.starts_with(&format!("[RCVBUF] p{i} ")), "path order: {l}");
            assert_eq!(field(l, "role="), role, "{l}");
            assert_eq!(field(l, "req=").parse::<usize>().unwrap(), REQ, "{l}");
            let granted: usize = field(l, "granted=").parse().unwrap();
            let via = field(l, "via=");
            assert!(via == "SO_RCVBUF" || via == "SO_RCVBUFFORCE", "{l}");
            let clamped = field(l, "clamped=");
            assert_eq!(clamped == "1", granted < 2 * REQ, "clamped= is the doubling shortfall: {l}");
            #[cfg(target_os = "linux")]
            assert!(granted >= floor(), "granted {granted} < floor {}: {l}", floor());
        }
    }

    #[cfg(target_os = "linux")]
    {
        let ctld: Vec<&str> = srv.lines().filter(|l| l.contains("[CTLD]")).collect();
        assert!(!ctld.is_empty(), "RWM_DIAG=1 receiver prints [CTLD]:\n{srv}");
        for p in 0..paths {
            let key = format!("rxdrop{p}=");
            let l = ctld
                .iter()
                .rev()
                .find(|l| l.contains(&key))
                .unwrap_or_else(|| panic!("no [CTLD] line carries {key}:\n{}", ctld.join("\n")));
            let d = field(l, &key);
            d.parse::<u64>().unwrap_or_else(|e| panic!("{key}{d} ({e}): {l}"));
        }
    }
}
