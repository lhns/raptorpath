//! fix/frontier-wedge — deterministic reproduction + regression gate for the
//! historic c3/C8 plain-mode "collapse run" (~2.2–3.3 Mbit/s for ~60 s,
//! self-resolving, cross-arm).
//!
//! PROVEN MECHANISM (forensics: /home/vibe/wedge-c.log, 2026-07-13 battery):
//! every wire symbol rides one ~1261–1275-byte QUIC datagram; quinn's
//! defaults are `initial_mtu = min_mtu = 1200`, so symbol datagrams are only
//! sendable because post-handshake PMTUD raises the path MTU to ~1452. A GE
//! loss burst of all-large packets looks to quinn's MTU BLACK-HOLE DETECTOR
//! exactly like an MTU black hole: it resets `current_mtu` to `min_mtu`
//! (1200) and pauses discovery for `black_hole_cooldown` (default 60 s).
//! During that window `max_datagram_size` (~1170) is smaller than every
//! symbol datagram, so EVERY data send — including every targeted retransmit
//! of the receiver's frontier blocker — fails at the sender with
//! `SendDatagramError::TooLarge` (8 077 consecutive failures in the captured
//! wedge), while small control datagrams keep the wire RTT fresh and the
//! path alive. The receiver's frontier freeze is the SYMPTOM; the sender's
//! MTU collapse is the disease. Self-resolution at ~60 s = the cooldown
//! expiring and PMTUD re-probing.
//!
//! THE FIX (`QuicTransport::apply_mtu_floor`): `min_mtu = initial_mtu =
//! 1350`, so a black-hole reset lands at a floor that still carries a full
//! symbol datagram. `RWM_MTU_FLOOR=0` restores stock quinn behavior (the
//! wedge-reproduction control arm).
//!
//! This file deliberately contains the env-touching tests in ONE process-
//! isolated integration binary (env is process-global; the control arm is
//! gated behind `RWM_WEDGE_CONTROL=1` so the default run stays short).
//!
//! The repro shapes the wire with an in-process lossy UDP proxy that drops
//! every UDP payload ≥ 1280 bytes for a 3-second window mid-transfer — a
//! REAL (transient) MTU black hole below quinn, which the L0 netem shim
//! structurally cannot express (it drops above quinn's packet layer, so
//! quinn never sees the large-packet loss pattern; that is why the wedge
//! never reproduced at L0).

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use raptorpath::{config, perf};
use tokio::net::UdpSocket;

/// UDP payload size (bytes) at/above which the proxy drops packets during
/// the black-hole window. Symbol datagrams are ~1275 bytes of QUIC payload
/// plus ~30 bytes of packet overhead (~1305 on the wire); control datagrams
/// (acks, pings, window acks) stay far below. QUIC Initial packets are
/// padded to exactly 1200 and pass, so the handshake is never affected.
const BIG: usize = 1280;

/// Number of big packets to let through before opening the black hole —
/// guarantees the hole opens mid-transfer, after PMTUD has raised the MTU
/// and bulk data is flowing.
const TRIGGER_BIG_PACKETS: u64 = 300;

/// Black-hole duration. Long enough for quinn's loss detection to declare
/// several all-large loss bursts (PTO-driven, sub-second on loopback) and
/// trip the black-hole detector; short relative to the 60 s cooldown whose
/// effect the arms discriminate.
const HOLE: Duration = Duration::from_secs(3);

struct ProxyState {
    /// Big packets forwarded so far (both directions).
    big_seen: AtomicU64,
    /// Micros-since-epoch when the hole opened (0 = not yet).
    hole_open_us: AtomicU64,
    epoch: Instant,
}

impl ProxyState {
    fn new() -> Self {
        Self {
            big_seen: AtomicU64::new(0),
            hole_open_us: AtomicU64::new(0),
            epoch: Instant::now(),
        }
    }

    /// Returns true if the packet should be DROPPED.
    fn drop_it(&self, len: usize) -> bool {
        if len < BIG {
            return false;
        }
        let now_us = self.epoch.elapsed().as_micros() as u64;
        let open = self.hole_open_us.load(Ordering::Relaxed);
        if open > 0 {
            return now_us.saturating_sub(open) < HOLE.as_micros() as u64;
        }
        let n = self.big_seen.fetch_add(1, Ordering::Relaxed) + 1;
        if n == TRIGGER_BIG_PACKETS {
            self.hole_open_us.store(now_us.max(1), Ordering::Relaxed);
            eprintln!("[proxy] black hole OPEN (3 s) after {n} big packets");
        }
        false
    }
}

/// Start a UDP proxy on `listen`: forwards client⇄`server`, dropping big
/// packets while the black hole is open. Returns the join handles (detached
/// — the test process ends them).
async fn spawn_proxy(listen: SocketAddr, server: SocketAddr) -> Arc<ProxyState> {
    let state = Arc::new(ProxyState::new());
    let client_side = Arc::new(UdpSocket::bind(listen).await.expect("proxy bind"));
    let server_side = Arc::new(UdpSocket::bind("127.0.0.1:0").await.expect("proxy bind 2"));
    server_side.connect(server).await.expect("proxy connect");

    // client → server
    {
        let state = state.clone();
        let client_side = client_side.clone();
        let server_side = server_side.clone();
        tokio::spawn(async move {
            let mut buf = vec![0u8; 65536];
            let mut client_addr: Option<SocketAddr> = None;
            loop {
                let Ok((len, from)) = client_side.recv_from(&mut buf).await else {
                    return;
                };
                // First sender on the client side IS the client; remember it
                // for the return direction (spawned lazily below).
                if client_addr.is_none() {
                    client_addr = Some(from);
                    let state = state.clone();
                    let client_side = client_side.clone();
                    let server_side = server_side.clone();
                    tokio::spawn(async move {
                        let mut buf = vec![0u8; 65536];
                        loop {
                            let Ok(len) = server_side.recv(&mut buf).await else {
                                return;
                            };
                            if state.drop_it(len) {
                                continue;
                            }
                            let _ = client_side.send_to(&buf[..len], from).await;
                        }
                    });
                }
                if state.drop_it(len) {
                    continue;
                }
                let _ = server_side.send(&buf[..len]).await;
            }
        });
    }
    state
}

async fn run_transfer(server_port: u16, proxy_port: u16, bytes: usize) -> Duration {
    let srv_cfg = config::RaptorpathConfig {
        server: Some(true),
        bind: Some(vec![format!("127.0.0.1:{server_port}")]),
        protocol_hint: Some("bulk".into()),
        window_reliable: Some(true),
        ..Default::default()
    };
    let (srv_pc, _) = config::resolve(&srv_cfg).unwrap();
    let srv = tokio::spawn(perf::server(srv_pc));
    tokio::time::sleep(Duration::from_millis(500)).await;

    let _proxy = spawn_proxy(
        format!("127.0.0.1:{proxy_port}").parse().unwrap(),
        format!("127.0.0.1:{server_port}").parse().unwrap(),
    )
    .await;

    let cli_cfg = config::RaptorpathConfig {
        bind: Some(vec!["127.0.0.1:0".into()]),
        peer: Some(vec![format!("127.0.0.1:{proxy_port}")]),
        protocol_hint: Some("bulk".into()),
        window_reliable: Some(true),
        ..Default::default()
    };
    let (cli_pc, _) = config::resolve(&cli_cfg).unwrap();

    let t0 = Instant::now();
    tokio::time::timeout(Duration::from_secs(150), perf::client(cli_pc, bytes, 1))
        .await
        .expect("transfer timed out (150 s)")
        .expect("perf client failed");
    let elapsed = t0.elapsed();
    srv.abort();
    elapsed
}

/// REGRESSION GATE (the fix, default env): a 3-second true MTU black hole
/// mid-transfer must NOT wedge the transfer for the 60-second quinn
/// black-hole cooldown. With the MTU floor, a black-hole reset lands at
/// 1350 — symbol datagrams stay sendable — so the transfer resumes the
/// moment the hole closes and completes in a few seconds.
///
/// Control arm (RWM_WEDGE_CONTROL=1 in env): stock quinn MTU behavior
/// (RWM_MTU_FLOOR=0). The same 3-second hole trips the detector, the MTU
/// collapses to 1200 < symbol datagram, and the transfer freezes until the
/// 60 s cooldown expires — asserted as elapsed > 45 s. Run it with:
///   RWM_WEDGE_CONTROL=1 cargo test --test mtu_blackhole_wedge --release -- --nocapture
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mtu_black_hole_does_not_wedge_transfer() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let control = std::env::var("RWM_WEDGE_CONTROL").map(|v| v == "1").unwrap_or(false);

    if control {
        // Wedge-reproduction arm: stock quinn MTU state machine.
        std::env::set_var("RWM_MTU_FLOOR", "0");
        let elapsed = run_transfer(47881, 47882, 8_000_000).await;
        eprintln!("[control arm] elapsed = {elapsed:?}");
        assert!(
            elapsed > Duration::from_secs(45),
            "control arm (stock quinn MTU) completed in {elapsed:?} — the 60 s \
             black-hole wedge did not reproduce; the fix's premise needs re-checking"
        );
        std::env::remove_var("RWM_MTU_FLOOR");
        return;
    }

    // Fix arm: MTU floor active (default).
    let elapsed = run_transfer(47883, 47884, 8_000_000).await;
    eprintln!("[fix arm] elapsed = {elapsed:?}");
    assert!(
        elapsed < Duration::from_secs(40),
        "transfer took {elapsed:?} despite the MTU floor — the 60 s black-hole \
         cooldown wedge is back (or the hole never closed)"
    );
}

/// Hard invariant behind the floor value: a maximum-size wire symbol batch
/// (1200-byte symbol + repair header + bincode/batch framing) must fit in a
/// QUIC datagram at `current_mtu == 1350` (the floor), i.e. its serialized
/// size must stay ≤ 1350 − 45 (conservative QUIC short-header + PN + AEAD
/// tag + DATAGRAM frame-header overhead — quinn's own budget is ~33).
#[test]
fn mtu_floor_covers_symbol_batch() {
    use raptorpath::fec::{FecBackend, WireSymbol};
    use raptorpath::transport::{SymbolBatch, WireMessage};

    // Worst case: repair symbol = 14-byte RLC repair header + 1200 coded.
    let repair = WireSymbol {
        block_id: u64::MAX,
        payload_id: u32::MAX,
        is_repair: true,
        data: vec![0xAB; 14 + 1200],
        backend: FecBackend::Rlc,
    };
    let msg = WireMessage::Data(SymbolBatch {
        symbols: vec![repair],
        send_timestamp_us: u64::MAX,
        batch_seq: u64::MAX,
        path_id: u32::MAX,
    });
    let wire = msg.serialize().expect("serialize");
    let budget = 1350 - 45;
    assert!(
        wire.len() <= budget,
        "serialized symbol batch is {} bytes > {} datagram budget at the 1350 \
         MTU floor — raise MTU_FLOOR in transport/quic.rs::apply_mtu_floor",
        wire.len(),
        budget
    );
    eprintln!(
        "symbol batch datagram = {} bytes; floor budget = {budget}",
        wire.len()
    );
}
