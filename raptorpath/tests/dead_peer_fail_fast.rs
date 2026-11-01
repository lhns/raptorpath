//! A client whose server dies (SIGKILL: no CONNECTION_CLOSE, a silent peer)
//! must fail fast: once every path's QUIC connection is lost (quinn's idle
//! timeout, `QUIC_IDLE_TIMEOUT`) the engine ends with "peer lost" and the
//! perf client exits non-zero, instead of running to the 300 s run timeout.
//! One lost path while another lives is not "lost" (the
//! `all_connections_lost` semantics; not exercised here).

#[path = "common/loopback.rs"]
mod loopback;

use loopback::{free_addrs, perf_args, spawn_perf_client, spawn_perf_server};
use raptorpath::net::REPORT_INTERVAL;
use raptorpath::transport::QUIC_IDLE_TIMEOUT;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

fn kill_server_mid_transfer(addrs: &[SocketAddr]) {
    // The red run (no fail-fast) ends at the DNF timeout: keep it bounded.
    let run_timeout = (3 * QUIC_IDLE_TIMEOUT).as_secs().to_string();
    let mut srv = spawn_perf_server(addrs, &[], &[]);
    // Large enough that the transfer is still under way when the server dies.
    let args = perf_args("bulk", "2000000000", "1");
    let mut cli = spawn_perf_client(addrs, &[("RWM_PERF_TIMEOUT_S", &run_timeout)], &args);
    if !cli.log.wait_line_after(0, "warmup", Duration::from_secs(60)) {
        panic!("client never finished its warm-up; it said:\n{}", cli.finish());
    }
    srv.kill();
    let killed = Instant::now();

    // Bound: the idle timeout plus ONE engine report interval of slack.
    let bound = QUIC_IDLE_TIMEOUT + REPORT_INTERVAL;
    let mut status = None;
    while killed.elapsed() < 3 * QUIC_IDLE_TIMEOUT {
        if let Some(s) = cli.exited() {
            status = Some(s);
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let took = killed.elapsed();
    let log = cli.finish();
    let status = status.unwrap_or_else(|| {
        panic!("client still running {took:?} after its server was killed (bound {bound:?}):\n{log}")
    });
    assert!(took <= bound, "client took {took:?} to exit, bound {bound:?} (status {status:?}):\n{log}");
    assert!(!status.success(), "client must exit non-zero on a dead peer ({status:?}):\n{log}");
    assert!(log.contains("peer lost"), "the peer-lost route must have executed:\n{log}");
}

#[test]
fn single_path_client_fails_fast_when_server_is_killed() {
    kill_server_mid_transfer(&free_addrs(1));
}

#[test]
fn two_path_client_fails_fast_when_server_is_killed() {
    kill_server_mid_transfer(&free_addrs(2));
}
