//! End to end at Realtime: small packets that the sender packs several to a
//! symbol (3 x 100 B, bursts) reach the server byte-equal, and also when the
//! one-shot `WindowStart` is lost (the F2 trigger). The seam observation
//! `TEST_MAX_PACKED` proves symbols really held several packets, so the test
//! cannot pass by sending one packet per symbol. Run with `--features test-seams`.
//! Own test binary: the seams and counters are process-global.

#[path = "common/loopback.rs"]
mod loopback;

use bytes::Bytes;
use raptorpath::net::emit_source::TEST_MAX_PACKED;
use raptorpath::net;
use raptorpath::transport::io_owner::TEST_DROP_WINDOW_START;
use raptorpath::tun::TunInterface;
use std::collections::HashSet;
use std::sync::atomic::Ordering::Relaxed;
use std::time::{Duration, Instant};

const GROUPS: u32 = 40;

fn packet(i: u32) -> Bytes {
    let mut p: Vec<u8> = (0..100).map(|j| ((i as usize * 13 + j) % 251) as u8 + 1).collect();
    p[..4].copy_from_slice(&i.to_be_bytes());
    Bytes::from(p)
}

async fn run_once(drop_window_start: bool) {
    let what = if drop_window_start { "window start dropped" } else { "window start kept" };
    TEST_MAX_PACKED.store(0, Relaxed);
    TEST_DROP_WINDOW_START.store(drop_window_start as u32, Relaxed);
    let (srv, cli) = loopback::in_process::cfgs(&[loopback::free_port()], "realtime", true);
    let (srv, cli) = (loopback::in_process::resolve(&srv), loopback::in_process::resolve(&cli));
    let _ = rustls::crypto::ring::default_provider().install_default();
    let binds = srv.bind_addrs.clone();
    let (srv_tun, mut srv_mem) = TunInterface::memory(1500);
    let (cli_tun, cli_mem) = TunInterface::memory(1500);
    let mut srv_task = tokio::spawn(net::run_with_tun(srv, srv_tun));
    loopback::in_process::wait_bound(&binds, &mut srv_task, what).await;
    let cli_task = tokio::spawn(net::run_with_tun(cli, cli_tun));

    // Bursts of three 100-byte packets, sent without yielding, so the sender's
    // packer (1 ms flush timeout) puts them in one 512-byte symbol.
    let sent: Vec<Bytes> = (0..GROUPS * 3).map(packet).collect();
    let feed = cli_mem.feed.clone();
    let to_send = sent.clone();
    let feeder = tokio::spawn(async move {
        for burst in to_send.chunks(3) {
            for p in burst {
                let _ = feed.send(p.clone()).await;
            }
            tokio::time::sleep(Duration::from_millis(3)).await;
        }
    });

    let mut got: HashSet<Bytes> = HashSet::new();
    let start = Instant::now();
    while got.len() < sent.len() {
        assert!(start.elapsed() < Duration::from_secs(30), "{what}: delivered {} of {}", got.len(), sent.len());
        tokio::select! {
            p = srv_mem.delivered.recv() => { if let Some(p) = p { got.insert(p); } }
            _ = tokio::time::sleep(Duration::from_millis(50)) => {}
        }
    }
    feeder.abort();
    cli_task.abort();
    srv_task.abort();

    for p in &sent {
        assert!(got.contains(p), "{what}: packet {} lost or altered", u32::from_be_bytes(p[..4].try_into().unwrap()));
    }
    assert!(TEST_MAX_PACKED.load(Relaxed) >= 3, "{what}: no symbol held 3 packets, so nothing was packed");
    assert_eq!(net::undeliverable_symbols(), 0, "{what}");
    if drop_window_start {
        assert_eq!(TEST_DROP_WINDOW_START.load(Relaxed), 0, "the seam must have dropped the WindowStart");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn packed_realtime_symbols_are_delivered_byte_equal_with_and_without_window_start() {
    run_once(false).await;
    run_once(true).await;
}
