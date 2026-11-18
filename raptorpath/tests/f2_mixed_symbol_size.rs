//! Option (ii) of ADR-0078: each direction's symbol size belongs to its
//! sender. Side A runs Bulk (S = 1200), side B Realtime (S = 512). B's decoder
//! must take A's 1200 so packets above 510 bytes (the most a 512-byte decoder
//! could ever return) arrive, and A's must take B's 512.
//!
//! Both sides get the same explicit `window_reliable = true`, so the test
//! isolates the size from the per-side rho default. Own test binary: the
//! counters are process-global.

#[path = "common/loopback.rs"]
mod loopback;

use bytes::Bytes;
use raptorpath::net::{self, RECV_SYMBOL_SIZES};
use raptorpath::tun::TunInterface;
use std::collections::{BTreeSet, HashSet};
use std::time::{Duration, Instant};

fn packet(i: u32, len: usize) -> Bytes {
    let mut p: Vec<u8> = (0..len).map(|j| ((i as usize * 7 + j) % 251) as u8 + 1).collect();
    p[..4].copy_from_slice(&i.to_be_bytes());
    Bytes::from(p)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn each_decoder_takes_its_peers_symbol_size_in_both_directions() {
    let (srv, mut cli) = loopback::in_process::cfgs(&[loopback::free_port()], "bulk", true);
    cli.protocol_hint = Some("realtime".into());
    let (srv, cli) = (loopback::in_process::resolve(&srv), loopback::in_process::resolve(&cli));
    assert_eq!((srv.window_reliable, cli.window_reliable), (true, true));
    let _ = rustls::crypto::ring::default_provider().install_default();
    let binds = srv.bind_addrs.clone();

    // A = server (Bulk), B = client (Realtime).
    let (a_tun, mut a_mem) = TunInterface::memory(1500);
    let (b_tun, mut b_mem) = TunInterface::memory(1500);
    let mut a_task = tokio::spawn(net::run_with_tun(srv, a_tun));
    loopback::in_process::wait_bound(&binds, &mut a_task, "mixed").await;
    let b_task = tokio::spawn(net::run_with_tun(cli, b_tun));

    // A -> B: 50 x 1196 B (A's TUN maximum) and 50 x 600 B, each indexed.
    // B -> A: 100 x 508 B (B's maximum).
    let a_feed = a_mem.feed.clone();
    let b_feed = b_mem.feed.clone();
    let sent_a: Vec<Bytes> = (0..100u32).map(|i| packet(i, if i < 50 { 1196 } else { 600 })).collect();
    let sent_b: Vec<Bytes> = (0..100u32).map(|i| packet(i, 508)).collect();
    let (sa, sb) = (sent_a.clone(), sent_b.clone());
    let f1 = tokio::spawn(async move {
        for p in sa {
            let _ = a_feed.send(p).await;
        }
    });
    let f2 = tokio::spawn(async move {
        for p in sb {
            let _ = b_feed.send(p).await;
        }
    });

    let (mut got_at_b, mut got_at_a): (HashSet<Bytes>, HashSet<Bytes>) = Default::default();
    let start = Instant::now();
    while got_at_b.len() < 100 || got_at_a.len() < 100 {
        assert!(
            start.elapsed() < Duration::from_secs(40),
            "mixed sizes: delivered {} of 100 at B and {} of 100 at A",
            got_at_b.len(),
            got_at_a.len()
        );
        tokio::select! {
            p = b_mem.delivered.recv() => { if let Some(p) = p { got_at_b.insert(p); } }
            p = a_mem.delivered.recv() => { if let Some(p) = p { got_at_a.insert(p); } }
            _ = tokio::time::sleep(Duration::from_millis(50)) => {}
        }
    }
    f1.abort();
    f2.abort();
    b_task.abort();
    a_task.abort();

    for p in &sent_a {
        assert!(got_at_b.contains(p), "A -> B: packet {} lost or altered", u32::from_be_bytes(p[..4].try_into().unwrap()));
    }
    for p in &sent_b {
        assert!(got_at_a.contains(p), "B -> A: packet {} lost or altered", u32::from_be_bytes(p[..4].try_into().unwrap()));
    }
    assert_eq!(net::undeliverable_symbols(), 0);
    let sizes: BTreeSet<(u16, u16)> = RECV_SYMBOL_SIZES.lock().unwrap().iter().copied().collect();
    assert_eq!(
        sizes,
        BTreeSet::from([(1200, 512), (512, 1200)]),
        "each decoder must take the peer's announced size"
    );
}
