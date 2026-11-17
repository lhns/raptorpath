//! Engine defect F2 (docs/status.md): a Realtime receiver misreads every
//! packed symbol when the one-shot, unreliable `WindowStart` is lost
//! (`window_packed` stays false), and still acks what it cannot unframe.
//! Red-first repro: the client's first `WindowStart` datagram is dropped at
//! the path owner (`TEST_DROP_WINDOW_START`), then a small object is sent.
//!
//! Own test binary: the seam and the undeliverable counter are
//! process-global.

#[path = "common/loopback.rs"]
mod loopback;

use raptorpath::transport::io_owner::TEST_DROP_WINDOW_START;
use std::sync::atomic::Ordering::Relaxed;
use std::time::Duration;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "F2: fails until the framing fix"]
async fn realtime_delivers_when_window_start_is_lost() {
    TEST_DROP_WINDOW_START.store(1, Relaxed);
    let (srv, cli) = loopback::in_process::cfgs(&[loopback::free_port()], "realtime", false);
    let (srv, cli) = (loopback::in_process::resolve(&srv), loopback::in_process::resolve(&cli));
    // The client returns Ok only after the warm-up object was acked and
    // fully received by the server app.
    loopback::in_process::run(srv, cli, 20_000, 1, Duration::from_secs(20), "f2 realtime").await;
    assert_eq!(TEST_DROP_WINDOW_START.load(Relaxed), 0, "the seam must have dropped the WindowStart");
}
