//! ρ = 1 receiver hand-off under a stalled consumer, through the real engine.
//!
//! Two in-process engines on memory TUNs (bulk, reliable window). The server
//! app stops reading until its inject channel (depth 8192) is FULL, then
//! drains. Every packet the client fed must arrive exactly once, in order:
//! the receiver holds on a full channel and never acks past what it holds
//! (`net/delivery.rs`). Mechanism liveness: the test proves the channel
//! actually saturated while more packets were still owed, so the hold path
//! (and the `reserve()` retry wake in `net/receiver.rs`) had to run.

#[path = "common/loopback.rs"]
mod loopback;

use std::time::Duration;

use bytes::{BufMut, Bytes, BytesMut};
use loopback::in_process;
use raptorpath::{net, tun::TunInterface};

const MAGIC_DATA: u16 = 0x5244; // "RD"
const MAGIC_WARM: u16 = 0x5257; // "RW"
/// Well above the 8192-deep memory inject channel, so it must saturate.
const N: u32 = 20_000;
const INJECT_DEPTH: usize = 8192;

fn pkt(magic: u16, idx: u32) -> Bytes {
    let mut b = BytesMut::with_capacity(64);
    b.put_u16(magic);
    b.put_u32(idx);
    b.put_bytes(0x5A, 58);
    b.freeze()
}

fn parse(p: &[u8]) -> Option<(u16, u32)> {
    if p.len() < 6 {
        return None;
    }
    Some((
        u16::from_be_bytes([p[0], p[1]]),
        u32::from_be_bytes([p[2], p[3], p[4], p[5]]),
    ))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn reliable_window_stalled_consumer_gets_every_packet_once_in_order() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let (srv, cli) = in_process::cfgs(&in_process::ports(1), "bulk", true);
    let (srv, cli) = (in_process::resolve(&srv), in_process::resolve(&cli));
    assert!(srv.window_reliable && cli.window_reliable, "must run the rho = 1 window");

    let binds = srv.bind_addrs.clone();
    let (srv_tun, mut srv_mem) = TunInterface::memory(1500);
    let mut srv_engine = tokio::spawn(net::run_with_tun(srv, srv_tun));
    in_process::wait_bound(&binds, &mut srv_engine, "reliable_delivery_hold server").await;
    let (cli_tun, cli_mem) = TunInterface::memory(1500);
    let cli_engine = tokio::spawn(net::run_with_tun(cli, cli_tun));
    let feed = cli_mem.feed.clone();

    // Warm-up: the tunnel passes traffic before the counted stream starts.
    let warm = tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            feed.send(pkt(MAGIC_WARM, 0)).await.expect("feed closed in warm-up");
            if let Ok(Some(p)) =
                tokio::time::timeout(Duration::from_millis(200), srv_mem.delivered.recv()).await
            {
                if parse(&p).map(|(m, _)| m) == Some(MAGIC_WARM) {
                    break;
                }
            }
        }
    })
    .await;
    assert!(warm.is_ok(), "tunnel never passed warm-up traffic");

    // The counted stream, fed from its own task (it back-pressures on the
    // client's feed channel once the server stops acking).
    let feeder = tokio::spawn(async move {
        for i in 0..N {
            if feed.send(pkt(MAGIC_DATA, i)).await.is_err() {
                panic!("feed closed at {i}");
            }
        }
    });

    // The consumer stalls until the inject channel is FULL...
    let saturated = tokio::time::timeout(Duration::from_secs(120), async {
        while srv_mem.delivered.len() < INJECT_DEPTH {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .is_ok();
    assert!(saturated, "the inject channel never filled: the hold path was not exercised");
    // ...and stays stalled a while longer, so the receiver holds (and the
    // sender's tail sweep retransmits the blocker) before the drain.
    tokio::time::sleep(Duration::from_millis(1500)).await;

    // Drain: every counted packet exactly once, in order.
    let mut next: u32 = 0;
    let drained = tokio::time::timeout(Duration::from_secs(180), async {
        while next < N {
            let p = srv_mem.delivered.recv().await.expect("server inject channel closed");
            match parse(&p) {
                Some((MAGIC_DATA, i)) => {
                    assert_eq!(i, next, "packet {i} delivered where {next} was owed (lost, duplicated or reordered)");
                    next += 1;
                }
                _ => {} // a late warm-up duplicate
            }
        }
    })
    .await;
    assert!(drained.is_ok(), "only {next} of {N} packets arrived: the rest were lost");
    feeder.await.expect("feeder");
    // Nothing more may follow (no duplicate of the tail).
    tokio::time::sleep(Duration::from_millis(500)).await;
    while let Ok(p) = srv_mem.delivered.try_recv() {
        if let Some((MAGIC_DATA, i)) = parse(&p) {
            panic!("packet {i} delivered twice");
        }
    }

    cli_engine.abort();
    srv_engine.abort();
}
