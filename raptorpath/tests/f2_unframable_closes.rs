//! Fail closed (ADR-0078): a source symbol the receiver cannot unframe is
//! counted, logged `[UNFRAMABLE]`, never delivered, and ends the receiver and
//! with it the tunnel. One behaviour at Bulk and at Realtime.
//!
//! The seam (`emit_source::TEST_CORRUPT_SEQ`) corrupts the K-th framed source
//! buffer BEFORE the encoder stores it, so every copy of that symbol (the
//! Realtime duplicate send, retransmits, every repair covering it) is the same
//! corrupted symbol and recovery cannot produce a clean one. Run with
//! `--features test-seams`. Own test binary: the seam and the counters are
//! process-global.

#[path = "common/loopback.rs"]
mod loopback;

use bytes::Bytes;
use raptorpath::net::emit_source::{
    TEST_CORRUPTED_AT, TEST_CORRUPTED_PAYLOAD, TEST_CORRUPT_COUNT, TEST_CORRUPT_SEQ,
};
use raptorpath::net::framing::read_packets;
use raptorpath::net::{self, UNDELIVERABLE_SYMBOLS};
use raptorpath::tun::TunInterface;
use std::collections::HashSet;
use std::io::Write;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const M: u32 = 200;
const PKT: usize = 300;
const K: u64 = 20;
const WARMUP: u32 = 10;

#[derive(Clone)]
struct Capture(Arc<Mutex<Vec<u8>>>);
impl Write for Capture {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn packet(i: u32) -> Bytes {
    let mut p = vec![(i % 251) as u8 + 1; PKT];
    p[..4].copy_from_slice(&i.to_be_bytes());
    Bytes::from(p)
}

async fn closes_at(hint: &str, log: &Capture) {
    UNDELIVERABLE_SYMBOLS.store(0, Relaxed);
    *TEST_CORRUPTED_PAYLOAD.lock().unwrap() = None;
    *TEST_CORRUPTED_AT.lock().unwrap() = None;
    TEST_CORRUPT_COUNT.store(0, Relaxed);
    TEST_CORRUPT_SEQ.store(K, Relaxed);
    log.0.lock().unwrap().clear();

    let (srv, cli) = loopback::in_process::cfgs(&[loopback::free_port()], hint, true);
    let (srv, cli) = (loopback::in_process::resolve(&srv), loopback::in_process::resolve(&cli));
    let _ = rustls::crypto::ring::default_provider().install_default();
    let binds = srv.bind_addrs.clone();
    let (srv_tun, mut srv_mem) = TunInterface::memory(1500);
    let (cli_tun, cli_mem) = TunInterface::memory(1500);
    let mut srv_task = tokio::spawn(net::run_with_tun(srv, srv_tun));
    loopback::in_process::wait_bound(&binds, &mut srv_task, hint).await;
    let cli_task = tokio::spawn(net::run_with_tun(cli, cli_tun));

    // Two 300-byte packets never share a 512-byte symbol, so each is one
    // source symbol at both hints and seq K is a single packet.
    // Warm-up: a few packets go first and the test waits until the server has
    // delivered one, so the close cannot beat every delivery (the engine's
    // delivery task is dropped with it). The seam fires at seq K > WARMUP.
    let mut delivered: HashSet<u32> = HashSet::new();
    for i in 0..WARMUP {
        cli_mem.feed.send(packet(i)).await.expect("client TUN open");
    }
    let first = tokio::time::timeout(Duration::from_secs(20), srv_mem.delivered.recv())
        .await
        .unwrap_or_else(|_| panic!("{hint}: the warm-up never reached the server"))
        .expect("server TUN open");
    delivered.insert(u32::from_be_bytes(first[..4].try_into().unwrap()));
    let feed = cli_mem.feed.clone();
    let feeder = tokio::spawn(async move {
        for i in WARMUP..M {
            if feed.send(packet(i)).await.is_err() {
                break;
            }
        }
    });

    let start = Instant::now();
    let ended = loop {
        assert!(start.elapsed() < Duration::from_secs(40), "{hint}: the server engine never ended");
        tokio::select! {
            p = srv_mem.delivered.recv() => {
                if let Some(p) = p {
                    delivered.insert(u32::from_be_bytes(p[..4].try_into().unwrap()));
                }
            }
            r = &mut srv_task => break (r, Instant::now()),
            _ = tokio::time::sleep(Duration::from_millis(50)) => {}
        }
    };
    feeder.abort();
    cli_task.abort();
    // The engine can end before the loop above drained everything it had
    // already delivered: drain the rest so the "not delivered" check sees all.
    while let Ok(p) = srv_mem.delivered.try_recv() {
        delivered.insert(u32::from_be_bytes(p[..4].try_into().unwrap()));
    }

    let saved = TEST_CORRUPTED_PAYLOAD.lock().unwrap().clone().expect("the seam fired");
    let (res, ended_at) = ended;
    assert!(res.expect("server task joined").is_ok(), "{hint}: the engine's fatal-receiver exit is Ok(())");
    // The seam stamps its own firing time (polling can miss a fast close).
    let corrupted_at = TEST_CORRUPTED_AT.lock().unwrap().expect("the seam stamped its firing time");
    assert!(
        ended_at.duration_since(corrupted_at) <= Duration::from_secs(5),
        "{hint}: the engine must end within 5 s of the corruption"
    );
    assert_eq!(net::undeliverable_symbols(), 1, "{hint}: exactly one unframable symbol");
    assert!(
        String::from_utf8_lossy(&log.0.lock().unwrap()).contains("[UNFRAMABLE]"),
        "{hint}: the always-on [UNFRAMABLE] line"
    );
    // The original packet of the corrupted symbol must not have been delivered.
    for pkt in read_packets(&saved).expect("the saved original is a valid symbol") {
        let idx = u32::from_be_bytes(pkt[..4].try_into().unwrap());
        assert!(!delivered.contains(&idx), "{hint}: packet {idx} of the corrupted symbol was delivered");
    }
    // It executed: the warm-up got through before the close.
    assert!(!delivered.is_empty(), "{hint}: nothing was delivered, so the close was not the fail-closed exit");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn unframable_symbol_closes_the_tunnel_at_bulk_and_realtime() {
    let log = Capture(Arc::new(Mutex::new(Vec::new())));
    let l2 = log.clone();
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::ERROR)
        .with_ansi(false)
        .with_writer(move || l2.clone())
        .init();
    closes_at("bulk", &log).await;
    closes_at("realtime", &log).await;
}
