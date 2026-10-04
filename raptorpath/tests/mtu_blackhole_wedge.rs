//! Deterministic reproduction of, and regression gate for, the plain-mode
//! MTU black-hole wedge (~2.2–3.3 Mbit/s for ~60 s, self-resolving; ADR-0055).
//!
//! Mechanism: every wire symbol rides one ~1261–1275-byte QUIC datagram; quinn's
//! defaults are `initial_mtu = min_mtu = 1200`, so symbol datagrams are only
//! sendable because post-handshake PMTUD raises the path MTU to ~1452. A GE
//! loss burst of all-large packets looks to quinn's MTU black-hole detector
//! exactly like an MTU black hole: it resets `current_mtu` to `min_mtu`
//! (1200) and pauses discovery for `black_hole_cooldown` (default 60 s).
//! During that window `max_datagram_size` (~1170) is smaller than every
//! symbol datagram, so every data send — including every targeted retransmit
//! of the receiver's frontier blocker — fails at the sender with
//! `SendDatagramError::TooLarge`, while small control datagrams keep the wire
//! RTT fresh and the path alive. The receiver's frontier freeze is the
//! symptom; the sender's MTU collapse is the cause. Self-resolution at ~60 s
//! is the cooldown expiring and PMTUD re-probing.
//!
//! The fix (`QuicTransport::apply_mtu_floor`): `min_mtu = initial_mtu =
//! 1350`, so a black-hole reset lands at a floor that still carries a full
//! symbol datagram. `RWM_MTU_FLOOR=0` restores stock quinn behavior (the
//! wedge-reproduction control arm).
//!
//! This file deliberately contains the env-touching tests in one process-
//! isolated integration binary (env is process-global; the control arm is
//! gated behind `RWM_WEDGE_CONTROL=1` so the default run stays short).
//!
//! The repro shapes the wire with an in-process lossy UDP proxy that drops
//! every UDP payload ≥ 1280 bytes for a 3-second window mid-transfer — a
//! real (transient) MTU black hole below quinn, which the L0 netem shim
//! structurally cannot express (it drops above quinn's packet layer, so
//! quinn never sees the large-packet loss pattern).

#[path = "common/loopback.rs"]
mod loopback;

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use loopback::in_process::{cfgs, ports, resolve, start_server};
use raptorpath::perf;
use tokio::net::UdpSocket;

/// The two tests in this binary share process-global env: the wedge test's
/// control arm WRITES `RWM_MTU_FLOOR`, and the serialization test reads the
/// wire-format gates. Each test holds this lock for its whole body, so no env
/// read ever races an env write.
static ENV_LOCK: Mutex<()> = Mutex::new(());

fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

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

/// Start a UDP proxy: forwards client⇄`server`, dropping big
/// packets while the black hole is open. Binds an OS-chosen loopback port and
/// returns it with the shared state; the relay tasks are detached — the test
/// process ends them.
async fn spawn_proxy(server: SocketAddr) -> (SocketAddr, Arc<ProxyState>) {
    let state = Arc::new(ProxyState::new());
    let client_side = Arc::new(UdpSocket::bind("127.0.0.1:0").await.expect("proxy bind"));
    let listen = client_side.local_addr().expect("proxy addr");
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
    (listen, state)
}

async fn run_transfer(bytes: usize) -> Duration {
    let (s, mut c) = cfgs(&ports(1), "bulk", true);
    let srv_pc = resolve(&s);
    let server = srv_pc.bind_addrs[0];
    let srv = start_server(srv_pc, "mtu black-hole transfer").await;

    // The client talks to the PROXY, never to the server directly.
    let (proxy, _state) = spawn_proxy(server).await;
    c.peer = Some(vec![proxy.to_string()]);
    let cli_pc = resolve(&c);

    let t0 = Instant::now();
    tokio::time::timeout(Duration::from_secs(150), perf::client(cli_pc, bytes, 1))
        .await
        .expect("transfer timed out (150 s)")
        .expect("perf client failed");
    let elapsed = t0.elapsed();
    srv.abort();
    elapsed
}

/// Regression gate (the fix, default env): a 3-second true MTU black hole
/// mid-transfer must not wedge the transfer for the 60-second quinn
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
// The env lock is held for the whole transfer ON PURPOSE (see `ENV_LOCK`).
#[allow(clippy::await_holding_lock)]
async fn mtu_black_hole_does_not_wedge_transfer() {
    let _env = env_lock();
    let _ = rustls::crypto::ring::default_provider().install_default();
    let control = std::env::var("RWM_WEDGE_CONTROL").map(|v| v == "1").unwrap_or(false);

    if control {
        // Wedge-reproduction arm: stock quinn MTU state machine.
        std::env::set_var("RWM_MTU_FLOOR", "0");
        let elapsed = run_transfer(8_000_000).await;
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
    let elapsed = run_transfer(8_000_000).await;
    eprintln!("[fix arm] elapsed = {elapsed:?}");
    // A REAL requirement, not a speed bound: the wedge this gates is quinn's
    // 60 s black-hole cooldown, so a regressed engine takes ≥ 60 s whatever
    // the host; the fixed one takes a few seconds. 40 s splits the two with
    // margin on either side.
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
    let _env = env_lock();
    use raptorpath::fec::{FecBackend, WireSymbol};
    use raptorpath::transport::{SymbolBatch, WireMessage};

    // Worst case: repair symbol = 14-byte RLC repair header + 1200 coded.
    let repair = WireSymbol {
        block_id: u64::MAX,
        payload_id: u32::MAX,
        is_repair: true,
        data: vec![0xAB; 14 + 1200].into(),
        backend: FecBackend::Rlc,
    };
    let msg = WireMessage::Data(SymbolBatch::new(vec![repair], u64::MAX, (u64::MAX, u64::MAX), u32::MAX));
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
