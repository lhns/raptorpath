//! The simulation is deterministic: the same seed gives a byte-identical
//! socket trace and engine log in separate processes, on one path and on
//! two (hash order bit only at N >= 2, sim-spike.md blocker 3); a different
//! seed gives a different trace; the server port is not an input (the same
//! seed on two ports gives the same trace, port numbers aside). Run with
//! `RUSTFLAGS="--cfg tokio_unstable" cargo test -p raptorpath --features sim,test-seams --test sim_determinism`.

mod sim_support;
use sim_support::{run, ChildRun, Scenario};

#[test]
fn child() {
    sim_support::child();
}

fn assert_same(a: &ChildRun, b: &ChildRun, what: &str) {
    assert!(!a.trace.is_empty(), "{what}: empty trace");
    assert_eq!(a.outcome, "client_ok", "{what}: transfer did not complete");
    if a.trace != b.trace {
        let n = a.trace.lines().zip(b.trace.lines()).take_while(|(x, y)| x == y).count();
        panic!(
            "{what}: traces differ from line {n}:\n  a: {:?}\n  b: {:?}",
            a.trace.lines().nth(n),
            b.trace.lines().nth(n)
        );
    }
    if a.log != b.log {
        let n = a.log.lines().zip(b.log.lines()).take_while(|(x, y)| x == y).count();
        panic!("{what}: logs differ from line {n}:\n  a: {:?}\n  b: {:?}", a.log.lines().nth(n), b.log.lines().nth(n));
    }
}

fn same_seed_twice(paths: u16, port: u16) {
    let a = run(&Scenario::basic(5, paths, port));
    let b = run(&Scenario::basic(5, paths, port));
    assert_same(&a, &b, &format!("{paths} path(s), seed 5"));
    let c = run(&Scenario::basic(6, paths, port));
    assert_eq!(c.outcome, "client_ok", "seed 6 did not complete");
    assert_ne!(a.trace, c.trace, "{paths} path(s): seeds 5 and 6 gave the same trace");
    println!(
        "paths={paths} events={} virt_s={:.3} wall_s={:.3}/{:.3}/{:.3}",
        a.trace.lines().count(),
        a.virt_s,
        a.wall_s,
        b.wall_s,
        c.wall_s
    );
}

/// `s` with every whole number equal to a server port (`ports`) replaced by
/// its index: traces and logs print the real server address.
fn unport(s: &str, ports: &[u16]) -> String {
    let mut out = String::with_capacity(s.len());
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i].is_ascii_digit() {
            let j = i + b[i..].iter().take_while(|c| c.is_ascii_digit()).count();
            let tok = &s[i..j];
            match ports.iter().position(|p| p.to_string() == tok) {
                Some(k) => out.push_str(&format!("SRV{k}")),
                None => out.push_str(tok),
            }
            i = j;
        } else {
            let c = s[i..].chars().next().unwrap();
            out.push(c);
            i += c.len_utf8();
        }
    }
    out
}

/// The server port is not a scenario input: the same seed on two ports
/// gives the same trace and log once the port numbers are masked. (The link
/// RNG streams used to be keyed by the port, so a port change redrew every
/// loss and jitter sample from the first datagram on.)
fn same_seed_two_ports(paths: u16, p1: u16, p2: u16) {
    let ports = |p: u16| (p..p + paths).collect::<Vec<_>>();
    let mask = |r: ChildRun, p: u16| ChildRun { trace: unport(&r.trace, &ports(p)), log: unport(&r.log, &ports(p)), ..r };
    let a = mask(run(&Scenario::basic(5, paths, p1)), p1);
    let b = mask(run(&Scenario::basic(5, paths, p2)), p2);
    assert_same(&a, &b, &format!("{paths} path(s), seed 5, ports {p1} and {p2}"));
}

#[test]
fn same_seed_two_ports_same_trace_one_path() {
    same_seed_two_ports(1, 47_130, 47_131);
}

#[test]
fn same_seed_two_ports_same_trace_two_paths() {
    same_seed_two_ports(2, 47_140, 47_150);
}

#[test]
fn same_seed_same_trace_one_path() {
    same_seed_twice(1, 47_100);
}

#[test]
fn same_seed_same_trace_two_paths() {
    same_seed_twice(2, 47_110);
}

/// Jitter witness: the link's jitter reaches the engine's RTT estimator, so
/// the recovery clock's sigma is not exactly zero (the spike's fixed-delay
/// link gave sigma 0). Same scenario with jitter off as the control.
#[test]
fn jitter_reaches_the_rtt_variance() {
    let sigma = |r: &ChildRun| -> Vec<f64> {
        r.log
            .lines()
            .filter(|l| l.starts_with("[QCLK] "))
            .filter_map(|l| l.split(' ').find_map(|f| f.strip_prefix("sigma_us_mean=")))
            .filter_map(|v| v.split('/').next()?.parse().ok())
            .collect()
    };
    let on = run(&Scenario::basic(5, 1, 47_120));
    let off = run(&Scenario { jitter_ms: 0, ..Scenario::basic(5, 1, 47_120) });
    let (s_on, s_off) = (sigma(&on), sigma(&off));
    println!("sigma_us_mean jitter=4ms {s_on:?}, jitter=0 {s_off:?}");
    assert!(!s_on.is_empty(), "no [QCLK] line: the recovery clock never evaluated (witness missing)");
    // With jitter off the sigma is not exactly 0 either (queueing and
    // retransmits at 1 % loss), so the witness is the ratio, not > 0.
    let max = |v: &[f64]| v.iter().cloned().fold(0.0, f64::max);
    assert!(max(&s_on) > 2.0 * max(&s_off), "jitter does not move the RTT sigma: on {s_on:?}, off {s_off:?}");
}
