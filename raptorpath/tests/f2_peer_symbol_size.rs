//! ADR-0078, option (ii): the peer's symbol size, announced in the handshake,
//! is checked on every path. Handshake sizes come back from `connect` and
//! `accept`; a runtime path is refused while no size is stored, when it
//! announces another size, and admitted when it matches; an engine without an
//! initial path is refused (the receiver has no size to build its decoder
//! with, and there is no fallback to the local one).

#[path = "common/loopback.rs"]
mod loopback;

use raptorpath::transport::QuicTransport;
use std::net::SocketAddr;

fn any() -> SocketAddr {
    "127.0.0.1:0".parse().unwrap()
}

/// A server transport announcing `size`, accepting its path 0 in the background.
async fn server(size: u16) -> (QuicTransport, SocketAddr) {
    let t = QuicTransport::new(&[any()], true, None, size).await.expect("server binds");
    let addr = t.local_addr(0).expect("path 0 bound");
    (t, addr)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn handshake_returns_the_peers_size_and_runtime_paths_are_checked() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    // connect/accept return what the PEER announced.
    let (s0, a0) = server(512).await;
    let s0 = std::sync::Arc::new(s0);
    let acc = { let s = s0.clone(); tokio::spawn(async move { s.accept(0).await }) };
    let c = QuicTransport::new(&[any()], false, None, 1200).await.expect("client binds");

    // While no peer size is stored, a runtime path is refused (before binding anything).
    let e = c.add_path(5, any(), Some(a0)).await.unwrap_err().to_string();
    assert!(e.contains("no peer symbol size"), "{e}");
    assert!(c.local_addr(5).is_none(), "the refused path must not be left behind");

    assert_eq!(c.connect(0, a0).await.expect("client connects"), 512, "client sees the server's size");
    assert_eq!(acc.await.unwrap().expect("server accepts"), 1200, "server sees the client's size");
    c.set_peer_symbol_size(512);

    // A runtime path announcing another size is refused and removed.
    let (s1, a1) = server(1200).await;
    let s1 = std::sync::Arc::new(s1);
    let acc1 = { let s = s1.clone(); tokio::spawn(async move { s.accept(0).await }) };
    let e = c.add_path(1, any(), Some(a1)).await.unwrap_err().to_string();
    assert!(e.contains("symbol size 1200") && e.contains("512"), "{e}");
    assert!(c.local_addr(1).is_none(), "the mismatching path is removed");
    let _ = acc1.await;

    // A runtime path announcing the established size is admitted.
    let (s2, a2) = server(512).await;
    let s2 = std::sync::Arc::new(s2);
    let acc2 = { let s = s2.clone(); tokio::spawn(async move { s.accept(0).await }) };
    c.add_path(2, any(), Some(a2)).await.expect("matching size is admitted");
    assert!(c.local_addr(2).is_some());
    acc2.await.unwrap().expect("server accepts the matching path");
}

/// An engine with no initial path cannot learn the peer's size: refused at
/// startup, at both roles, instead of decoding with the local size.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_engine_without_an_initial_path_is_refused() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    for server in [true, false] {
        let (mut srv, mut cli) = loopback::in_process::cfgs(&[loopback::free_port()], "bulk", true);
        let cfg = if server { &mut srv } else { &mut cli };
        cfg.bind = Some(vec![]);
        cfg.peer = Some(vec![]);
        let cfg = loopback::in_process::resolve(cfg);
        let (tun, _mem) = raptorpath::tun::TunInterface::memory(1500);
        let e = tokio::time::timeout(std::time::Duration::from_secs(10), raptorpath::net::run_with_tun(cfg, tun))
            .await
            .expect("the engine must refuse at once, not idle")
            .unwrap_err()
            .to_string();
        assert!(e.contains("no initial path"), "server={server}: {e}");
    }
}
