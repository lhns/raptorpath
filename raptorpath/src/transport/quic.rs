//! QUIC transport implementation using quinn.
//!
//! Each path gets its own QUIC connection. We use:
//! - DATAGRAM frames for symbol data (unreliable, low overhead)
//! - A bidirectional stream for control messages (reliable)
//!
//! TLS modes:
//! - Default: self-signed cert, skip verification (dev/testing)
//! - Pinned: verify server cert matches a pinned DER/PEM file (production)

use super::l0_netem::L0Netem;
use super::rcvbuf::{self, RcvbufGrant};
use super::protocol::{ControlMessage, Handshake, PROTOCOL_VERSION, SymbolBatch, WireMessage};
use crate::scheduler::PathId;
use dashmap::DashMap;
use quinn::{ClientConfig, Endpoint, ServerConfig};
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
use sha2::{Digest, Sha256};
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

// ───────────────────────────────────────────────────────────────────────────
// QUIC substrate congestion-controller override (env `RWM_QUIC_CC`, default
// unset ⇒ quinn BBR; ADR-0054, paper §8.4).
//
// Why: quinn gates every packet send — including the DATAGRAM frames that
// carry all raptorpath wire symbols — on its own congestion window
// (quinn-proto connection/mod.rs "blocked by congestion control"). A
// loss-reactive Cubic window is a hard per-connection (= per-path) ceiling
// underneath raptorpath's loss-tolerant FEC/CC design on a GE-lossy path,
// so the default is BBR and Cubic is the explicit opt-out. BBR takes a
// 0.95–0.96 share against a competing Cubic flow — mildly aggressive,
// within the deployed-BBRv1 envelope.
//   RWM_QUIC_CC=bbr          quinn BBR (explicit; = the default)
//   RWM_QUIC_CC=bbr_rs       in-tree burst-robust BBR (transport/bbr_rs.rs:
//                            quinn's Bbr with the interval-guarded per-flight
//                            rate sampler; ADR-0054/0061; an experiment arm)
//   RWM_QUIC_CC=newreno
//   RWM_QUIC_CC=cubic        quinn stock Cubic (the fairness arm)
//   RWM_QUIC_CC=passthrough  our engine owns the window (see below)
// Unrecognized values warn and keep the BBR default.
// Applied to both client and server configs (each direction's sends are
// governed by the sender-side controller of that connection).
//
// Passthrough (ADR-0062): substrate CC as policy. quinn's controller
// becomes a pass-through shim whose window() simply reads an
// Arc<AtomicU64> (bytes) that the raptorpath engine writes per path — the
// engine's own Copa-lite per-path cwnd becomes the congestion window of the
// substrate (per connection = per path), instead of min(app CC, quinn CC)
// double control. quinn's loss events are recorded (stats only), never acted
// on — loss handling is the FEC layer's job (paper §8); congestion safety is
// Copa's delay backoff, which the engine writes into the atomic. quinn's own
// pacer derives its rate from this window, so pacing stays consistent with
// the engine's cwnd. The atomic starts at PASSTHROUGH_INITIAL_WINDOW so the
// TLS handshake and pre-feed traffic are never starved before Copa's first
// cwnd write; connections that never get a Copa feed (ack-only reverse
// direction) simply keep that static window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum QuicCcMode {
    /// Env unset/unrecognized/explicit `bbr`: quinn BBR — the shipped default.
    Bbr,
    /// Explicit `bbr_rs`: the in-tree burst-robust BBR (one changed
    /// mechanism vs quinn's: the bandwidth estimator — see
    /// transport/bbr_rs.rs module docs).
    BbrRs,
    NewReno,
    /// Explicit `cubic`: quinn stock Cubic — the fairness arm.
    Cubic,
    Passthrough,
}

fn quic_cc_mode() -> QuicCcMode {
    let Some(name) = crate::gates::get().quic_cc.clone() else {
        return QuicCcMode::Bbr;
    };
    match name.trim().to_ascii_lowercase().as_str() {
        "bbr" => QuicCcMode::Bbr,
        "bbr_rs" => QuicCcMode::BbrRs,
        "newreno" => QuicCcMode::NewReno,
        "cubic" => QuicCcMode::Cubic,
        "passthrough" => QuicCcMode::Passthrough,
        other => {
            warn!(%other, "RWM_QUIC_CC unrecognized — keeping the BBR default");
            QuicCcMode::Bbr
        }
    }
}

/// Initial pass-through window (bytes). Generous on purpose: it covers the
/// TLS handshake and the first RTTs before the engine's first Copa cwnd
/// write (a starved handshake would deadlock the tunnel), and it is the
/// permanent window for connections whose direction carries only control
/// traffic (no Copa feed). Once the engine writes, Copa owns the value.
const PASSTHROUGH_INITIAL_WINDOW: u64 = 256 * 1024;

/// Absolute floor for the pass-through window: never below two datagrams, so
/// a zero/garbage write can never wedge the connection entirely (ACK and
/// control packets keep flowing at a trickle).
const PASSTHROUGH_MIN_WINDOW_MTUS: u64 = 2;

/// Record-only counters for what quinn would have reacted to (RWM_DIAG-class
/// observability; never gates anything).
#[derive(Debug, Default)]
pub struct PassthroughCcStats {
    pub congestion_events: std::sync::atomic::AtomicU64,
    pub lost_bytes: std::sync::atomic::AtomicU64,
    pub persistent_congestion: std::sync::atomic::AtomicU64,
}

/// The pass-through `quinn::congestion::Controller`: `window()` reads the
/// shared atomic; every congestion signal is a recorded no-op.
struct PassthroughController {
    window: Arc<std::sync::atomic::AtomicU64>,
    stats: Arc<PassthroughCcStats>,
    mtu: u16,
}

impl quinn::congestion::Controller for PassthroughController {
    fn on_congestion_event(
        &mut self,
        _now: std::time::Instant,
        _sent: std::time::Instant,
        is_persistent_congestion: bool,
        lost_bytes: u64,
    ) {
        use std::sync::atomic::Ordering;
        self.stats.congestion_events.fetch_add(1, Ordering::Relaxed);
        self.stats.lost_bytes.fetch_add(lost_bytes, Ordering::Relaxed);
        if is_persistent_congestion {
            self.stats.persistent_congestion.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn on_mtu_update(&mut self, new_mtu: u16) {
        self.mtu = new_mtu;
    }

    fn window(&self) -> u64 {
        self.window
            .load(std::sync::atomic::Ordering::Relaxed)
            .max(PASSTHROUGH_MIN_WINDOW_MTUS * self.mtu as u64)
    }

    fn clone_box(&self) -> Box<dyn quinn::congestion::Controller> {
        Box::new(Self {
            window: self.window.clone(),
            stats: self.stats.clone(),
            mtu: self.mtu,
        })
    }

    fn initial_window(&self) -> u64 {
        PASSTHROUGH_INITIAL_WINDOW
    }

    fn into_any(self: Box<Self>) -> Box<dyn std::any::Any> {
        self
    }
}

/// Factory handing every connection built from it the same per-path window
/// atomic (one factory per path/endpoint — per-connection = per-path).
struct PassthroughFactory {
    window: Arc<std::sync::atomic::AtomicU64>,
    stats: Arc<PassthroughCcStats>,
}

impl quinn::congestion::ControllerFactory for PassthroughFactory {
    fn build(
        self: Arc<Self>,
        _now: std::time::Instant,
        current_mtu: u16,
    ) -> Box<dyn quinn::congestion::Controller> {
        Box::new(PassthroughController {
            window: self.window.clone(),
            stats: self.stats.clone(),
            mtu: current_mtu,
        })
    }
}

fn quic_cc_factory(
) -> Option<Arc<dyn quinn::congestion::ControllerFactory + Send + Sync + 'static>> {
    match quic_cc_mode() {
        QuicCcMode::Bbr => {
            info!("quinn congestion controller: BBR (shipped default; RWM_QUIC_CC overrides)");
            Some(Arc::new(quinn::congestion::BbrConfig::default()))
        }
        QuicCcMode::BbrRs => {
            // Mechanism-liveness echo (docs/measurement-discipline.md rule
            // 1): the battery greps for "burst-robust BBR".
            info!(
                "RWM_QUIC_CC=bbr_rs: burst-robust BBR (in-tree controller, \
                 interval-guarded per-flight rate sampler — ADR-0061 family)"
            );
            Some(Arc::new(super::bbr_rs::BbrRsConfig::default()))
        }
        QuicCcMode::NewReno => {
            info!("RWM_QUIC_CC=newreno: quinn congestion controller overridden to NewReno");
            Some(Arc::new(quinn::congestion::NewRenoConfig::default()))
        }
        QuicCcMode::Cubic => {
            info!("RWM_QUIC_CC=cubic: quinn stock Cubic (legacy wire / fairness arm)");
            Some(Arc::new(quinn::congestion::CubicConfig::default()))
        }
        // Passthrough needs a per-path handle — built in cc_factory_for_path.
        QuicCcMode::Passthrough => None,
    }
}

/// A QUIC-based multipath transport.
///
/// Uses DashMap for connections so paths can be added/removed at runtime.
pub struct QuicTransport {
    /// Local endpoints (one per bind address / path)
    endpoints: DashMap<PathId, Endpoint>,
    /// Active connections per path
    connections: DashMap<PathId, quinn::Connection>,
    /// Whether this transport is a server
    is_server: bool,
    /// Optional pinned certificate for client-side verification.
    /// When set, the client verifies the server's cert matches this fingerprint.
    pinned_cert_hash: Option<[u8; 32]>,
    /// L0 netem shim (env `RWM_L0_NETEM`; None = shipped path, byte-identical).
    l0_netem: Option<Arc<L0Netem>>,
    /// `RWM_QUIC_CC=passthrough`: the engine owns the substrate window.
    cc_passthrough: bool,
    /// Per-path pass-through window handles (bytes) — one per endpoint/path,
    /// created when that path's endpoint config is built; the engine writes
    /// its Copa cwnd here via `set_cc_window_bytes`.
    cc_windows: DashMap<PathId, Arc<std::sync::atomic::AtomicU64>>,
    /// Per-path record-only pass-through congestion stats (diagnostics).
    cc_stats: DashMap<PathId, Arc<PassthroughCcStats>>,
    /// `RWM_DIAG`: run the datagram send-queue audit at the
    /// `send_datagram_shaped` seam. Resolved once, here, so the shipped
    /// default pays neither the extra connection-lock take nor the atomics.
    dg_audit: bool,
    /// Datagram send-queue audit counters, per path. See
    /// `datagram_queue_stats`.
    dg_stats: DashMap<PathId, Arc<DatagramQueueAudit>>,
    /// `RWM_DIAG` receive-side audit: datagrams the app read from quinn
    /// (`read_datagram` Ok), per path. Against quinn's own
    /// `frame_rx.datagram` it names what quinn's bounded incoming buffer
    /// evicted before the app saw it. See `datagram_rx_audit`.
    dg_rx_read: DashMap<PathId, Arc<std::sync::atomic::AtomicU64>>,
    /// Per path: a clone of the endpoint's UDP socket (same kernel socket,
    /// second fd), kept to read the socket's live `SO_RCVBUF` and its
    /// kernel drop count (`SO_MEMINFO`) — quinn does not expose the fd.
    /// Removed with the path so the port is released with the endpoint.
    rx_probe: DashMap<PathId, std::net::UdpSocket>,
    /// Per path: the `SO_RCVBUF` request and the read-back grant at bind
    /// (transport/rcvbuf.rs; echoed once as `[RCVBUF]`).
    rcvbuf_grants: DashMap<PathId, RcvbufGrant>,
}

/// Bind one path's endpoint socket with the receive-buffer request
/// (transport/rcvbuf.rs), echo the grant, and hand the socket to quinn.
/// Returns the endpoint, a probe clone of its socket, and the grant.
fn bind_endpoint(
    path_id: PathId,
    addr: SocketAddr,
    server_config: Option<ServerConfig>,
) -> anyhow::Result<(Endpoint, std::net::UdpSocket, RcvbufGrant)> {
    let (sock, grant) = rcvbuf::bind_udp_with_rcvbuf(addr, rcvbuf::RCVBUF_REQUEST)?;
    let probe = sock.try_clone()?;
    let role = if server_config.is_some() { "server" } else { "client" };
    let local = sock.local_addr().unwrap_or(addr);
    crate::readout!("{}", rcvbuf::echo_line(path_id as u32, role, local, &grant));
    let runtime = quinn::default_runtime()
        .ok_or_else(|| anyhow::anyhow!("no async runtime found"))?;
    let ep = Endpoint::new(quinn::EndpointConfig::default(), server_config, sock, runtime)?;
    Ok((ep, probe, grant))
}

/// The datagram send-queue audit (`RWM_DIAG`), one per path.
///
/// **The gap this closes.** `quinn::Connection::send_datagram` calls
/// `Datagrams::send(data, drop = true)` (quinn-proto 0.11.14,
/// `connection/datagrams.rs`:38–48), which silently evicts the oldest
/// queued datagrams when the 4 MB send buffer overflows — about 3 300
/// 1 200 B symbols — logging a `trace!` nobody enables and returning `Ok`.
/// The engine's `src=`/`cod=` gauges count handoffs, not transmissions, so
/// an evicted symbol is indistinguishable from a delivered one. Store caps
/// of a few thousand symbols are the same order as that buffer.
///
/// **It cannot be counted exactly from quinn's public API, and this is what
/// is available instead.** quinn exposes no eviction counter and no hook;
/// `ConnectionStats` counts frames transmitted, not datagrams dropped before
/// transmission. What it does expose is
/// `Connection::datagram_send_buffer_space()`, which is
/// `datagram_send_buffer_size.saturating_sub(outgoing_total)`. Read at the
/// seam, immediately before the send, that gives an exact predicate:
///
/// * quinn's eviction loop runs iff `outgoing_total > buffer_size` on entry;
/// * `space == 0` iff `outgoing_total >= buffer_size` on entry.
///
/// So `full` counts every call that evicted, plus the measure-zero tie where
/// the queue is byte-exactly full. **`full` is therefore an upper bound on
/// the number of evicting calls that is tight to one boundary case, and a
/// lower bound on the number of datagrams evicted** — one call's `while`
/// loop pops until it is back under the ceiling, which is one datagram when
/// sizes are uniform (ours are: 1 200 B symbols) and more when they are not.
/// Both directions are named because neither is exact.
///
/// The corroborating cross-check is independent of that predicate:
/// `tx_frames` is quinn's own `stats().frame_tx.datagram`, the count of
/// DATAGRAM frames actually put on the wire. They are never
/// retransmitted, so in a run that ends with a drained queue
/// `handoff − tx_frames` is the total lost to eviction, computed without
/// reference to `full` at all. `space` is echoed so the queue depth at the
/// last DIAG window is readable and `handoff − tx_frames` can be corrected
/// for what was still queued.
///
/// **Off-value property:** with `RWM_DIAG` unset the audit does not run and
/// every counter reads 0, so `dgq[...]` never appears — enforced by
/// `transport::quic::tests::datagram_queue_audit_is_off_without_diag`.
///
/// **Scope:** only the real `conn.send_datagram` path is audited. The
/// `RWM_L0_NETEM` shim branch has its own transit ledger
/// (`l0_transit_stats`) and does not touch quinn's datagram buffer at all.
#[derive(Default)]
pub struct DatagramQueueAudit {
    /// Calls into the seam that quinn accepted (returned `Ok`).
    pub handoff: std::sync::atomic::AtomicU64,
    /// Calls whose `datagram_send_buffer_space()` was 0 on entry — the
    /// eviction predicate. See the type doc for exactly what it bounds.
    pub full: std::sync::atomic::AtomicU64,
    /// Calls quinn rejected (`TooLarge` / `UnsupportedByPeer` / `Disabled`).
    /// These are loud (the seam returns `Err`); counted so that
    /// `handoff + err` reconciles with the engine's own handoff count.
    pub err: std::sync::atomic::AtomicU64,
    /// `datagram_send_buffer_space()` at the most recent call, in bytes.
    pub space: std::sync::atomic::AtomicU64,
}

impl QuicTransport {
    /// Create a new transport with endpoints bound to the given addresses.
    ///
    /// `pin_cert_path`: optional path to a DER or PEM certificate file.
    /// When provided, the client will verify that the server's certificate
    /// matches this pinned cert (SHA-256 fingerprint comparison).
    pub async fn new(
        bind_addrs: &[SocketAddr],
        is_server: bool,
        pin_cert_path: Option<&Path>,
    ) -> anyhow::Result<Self> {
        let pinned_cert_hash = pin_cert_path
            .map(|p| load_pinned_cert_hash(p))
            .transpose()?;

        if let Some(hash) = &pinned_cert_hash {
            info!(fingerprint = %hex::encode(hash), "TLS cert pinning enabled");
        }

        let cc_passthrough = quic_cc_mode() == QuicCcMode::Passthrough;
        if cc_passthrough {
            info!(
                initial_window = PASSTHROUGH_INITIAL_WINDOW,
                "RWM_QUIC_CC=passthrough: quinn congestion window is engine-owned (per path)"
            );
        }
        let cc_windows: DashMap<PathId, Arc<std::sync::atomic::AtomicU64>> = DashMap::new();
        let cc_stats: DashMap<PathId, Arc<PassthroughCcStats>> = DashMap::new();

        let endpoints = DashMap::new();
        let rx_probe = DashMap::new();
        let rcvbuf_grants = DashMap::new();

        for (i, addr) in bind_addrs.iter().enumerate() {
            let cc = Self::cc_factory_for_path(
                cc_passthrough,
                &cc_windows,
                &cc_stats,
                i as PathId,
            );
            let endpoint = if is_server {
                let (server_config, cert_der) = Self::generate_self_signed_config(cc)?;
                // Log the server cert fingerprint so the user can pin it on the client
                let fingerprint = sha256_fingerprint(&cert_der[0]);
                info!(%addr, path_id = i, fingerprint = %hex::encode(fingerprint),
                    "server endpoint bound — use this fingerprint for --pin-cert");
                bind_endpoint(i as PathId, *addr, Some(server_config))?
            } else {
                let (mut ep, probe, grant) = bind_endpoint(i as PathId, *addr, None)?;
                let client_config = Self::make_client_config(pinned_cert_hash, cc);
                ep.set_default_client_config(client_config);
                info!(%addr, path_id = i, "client endpoint bound");
                (ep, probe, grant)
            };
            let (endpoint, probe, grant) = endpoint;
            endpoints.insert(i as PathId, endpoint);
            rx_probe.insert(i as PathId, probe);
            rcvbuf_grants.insert(i as PathId, grant);
        }

        Ok(Self {
            endpoints,
            connections: DashMap::new(),
            is_server,
            pinned_cert_hash,
            l0_netem: L0Netem::from_env(),
            cc_passthrough,
            cc_windows,
            cc_stats,
            // Resolved once, at construction: the audit's per-datagram cost
            // (one connection-lock take for `datagram_send_buffer_space`)
            // must not exist on the shipped path. `RWM_DIAG` is already in
            // `RWM_FORWARD` and the `[GATES]` echo, so this adds no name to
            // the gate surface.
            dg_audit: crate::gates::get().diag,
            dg_stats: DashMap::new(),
            dg_rx_read: DashMap::new(),
            rx_probe,
            rcvbuf_grants,
        })
    }

    /// The `SO_RCVBUF` request and the kernel's grant for `path_id`'s
    /// endpoint socket, read back at bind.
    pub fn rcvbuf_grant(&self, path_id: PathId) -> Option<RcvbufGrant> {
        self.rcvbuf_grants.get(&path_id).map(|g| *g)
    }

    /// `path_id`'s endpoint socket's live `SO_RCVBUF` (getsockopt now).
    pub fn rx_socket_rcvbuf(&self, path_id: PathId) -> Option<usize> {
        rcvbuf::recv_buffer_size(&*self.rx_probe.get(&path_id)?).ok()
    }

    /// `path_id`'s endpoint socket's kernel drop count (`sk_drops`,
    /// cumulative; unit: skbs, i.e. GRO superpackets — transport/rcvbuf.rs).
    /// `None` off Linux or without a socket; never a substituted zero.
    pub fn rx_socket_drops(&self, path_id: PathId) -> Option<u64> {
        rcvbuf::socket_drops(&*self.rx_probe.get(&path_id)?)
    }

    /// Per-path congestion-controller factory. Passthrough mode gets a
    /// per-path factory sharing that path's window atomic (per-connection =
    /// per-path: each endpoint serves exactly one path, and any reconnect on
    /// it correctly inherits the same engine-owned window); the other modes
    /// use the stock env-selected factory.
    fn cc_factory_for_path(
        cc_passthrough: bool,
        cc_windows: &DashMap<PathId, Arc<std::sync::atomic::AtomicU64>>,
        cc_stats: &DashMap<PathId, Arc<PassthroughCcStats>>,
        path_id: PathId,
    ) -> Option<Arc<dyn quinn::congestion::ControllerFactory + Send + Sync + 'static>> {
        if !cc_passthrough {
            return quic_cc_factory();
        }
        let window = cc_windows
            .entry(path_id)
            .or_insert_with(|| {
                Arc::new(std::sync::atomic::AtomicU64::new(PASSTHROUGH_INITIAL_WINDOW))
            })
            .clone();
        let stats = cc_stats
            .entry(path_id)
            .or_insert_with(|| Arc::new(PassthroughCcStats::default()))
            .clone();
        Some(Arc::new(PassthroughFactory { window, stats }))
    }

    /// Engine write side of the pass-through window: set path `path_id`'s
    /// substrate congestion window in bytes. No-op unless
    /// `RWM_QUIC_CC=passthrough` created a handle for this path.
    pub fn set_cc_window_bytes(&self, path_id: PathId, bytes: u64) {
        if !self.cc_passthrough {
            return;
        }
        if let Some(w) = self.cc_windows.get(&path_id) {
            w.store(bytes, std::sync::atomic::Ordering::Relaxed);
        }
    }

    /// Whether the pass-through substrate CC is active (the engine owns the
    /// per-path quinn window and should be feeding it).
    pub fn cc_passthrough_active(&self) -> bool {
        self.cc_passthrough
    }

    /// Packet-timed path RTT for `path_id` from quinn's RFC 9002 estimator
    /// (ADR-0062): measured at the QUIC packet layer — send of an
    /// ack-eliciting packet to receipt of its ACK, ack-delay corrected — so
    /// it excludes the sender's own app-layer store/reservoir dwell in the
    /// datagram queue (which sits before packetization). This is the
    /// wire clock for Copa's queue term d_q = wire_rtt − wire_RTTmin; the
    /// app-layer echo RTT stays with the reliability/tail machinery where
    /// end-to-end (pipeline-inclusive) delay is the right quantity.
    pub fn wire_rtt(&self, path_id: PathId) -> Option<std::time::Duration> {
        self.connections.get(&path_id).map(|c| c.rtt())
    }

    /// Quinn-level DATAGRAM frame counters for `path_id`:
    /// `(datagram_frames_rx, datagram_frames_tx)` from `Connection::stats()`.
    ///
    /// Wedge forensics: `frame_rx.datagram` counts every DATAGRAM frame quinn
    /// accepted at the packet layer — before the app's `read_datagram()` and
    /// before quinn's bounded incoming datagram buffer (which silently drops
    /// the oldest buffered datagram on overflow). If
    /// this counter advances while the app-level receive loop sees nothing,
    /// arriving datagrams are being destroyed between quinn's packet layer
    /// and the application (buffer overflow), not lost on the wire.
    pub fn datagram_frame_stats(&self, path_id: PathId) -> Option<(u64, u64)> {
        self.connections.get(&path_id).map(|c| {
            let s = c.stats();
            (s.frame_rx.datagram, s.frame_tx.datagram)
        })
    }

    /// Quinn substrate congestion gauge for `path_id` (a diagnosis
    /// instrument — read only at the RWM_DIAG print, never gates anything):
    /// `(cwnd_bytes, congestion_events, lost_packets, sent_packets)` from
    /// `Connection::stats().path`. Under the shipped BBR default the cwnd
    /// IS 2 × quinn's internal BtlBŵ × RTprop, so a cwnd many multiples of
    /// the true BDP·MTU is direct in-vivo evidence of the max-filter
    /// over-read.
    pub fn quinn_path_stats(&self, path_id: PathId) -> Option<(u64, u64, u64, u64)> {
        self.connections.get(&path_id).map(|c| {
            let p = c.stats().path;
            (p.cwnd, p.congestion_events, p.lost_packets, p.sent_packets)
        })
    }

    /// L0 shim transit counters, None when the shim is off —
    /// (enq, ge_drops, tail_drops, sent_ok, send_errs, queued_now).
    pub fn l0_transit_stats(&self) -> Option<(u64, u64, u64, u64, u64, usize)> {
        self.l0_netem.as_ref().map(|s| s.transit_stats())
    }

    /// Read back the pass-through window (bytes) for diagnostics; None when
    /// passthrough is off or the path has no handle.
    pub fn cc_window_bytes(&self, path_id: PathId) -> Option<u64> {
        self.cc_windows
            .get(&path_id)
            .map(|w| w.load(std::sync::atomic::Ordering::Relaxed))
    }

    /// Record-only pass-through congestion stats for diagnostics:
    /// (congestion_events, lost_bytes, persistent_congestion).
    pub fn cc_passthrough_stats(&self, path_id: PathId) -> Option<(u64, u64, u64)> {
        use std::sync::atomic::Ordering;
        self.cc_stats.get(&path_id).map(|s| {
            (
                s.congestion_events.load(Ordering::Relaxed),
                s.lost_bytes.load(Ordering::Relaxed),
                s.persistent_congestion.load(Ordering::Relaxed),
            )
        })
    }

    /// Datagram send seam: the L0 netem shim (when active) shapes + schedules
    /// the send; otherwise this is exactly `conn.send_datagram`.
    fn send_datagram_shaped(
        &self,
        path_id: PathId,
        conn: &quinn::Connection,
        data: bytes::Bytes,
    ) -> anyhow::Result<()> {
        match &self.l0_netem {
            Some(shim) => {
                shim.send(path_id, self.is_server, conn, data);
                Ok(())
            }
            None => {
                if !self.dg_audit {
                    conn.send_datagram(data)?;
                    return Ok(());
                }
                use std::sync::atomic::Ordering::Relaxed;
                // Read the queue depth before the send: quinn's eviction
                // decision is taken against the state on entry, so this is
                // the only moment at which the predicate is meaningful.
                let space = conn.datagram_send_buffer_space() as u64;
                let a = self
                    .dg_stats
                    .entry(path_id)
                    .or_insert_with(|| Arc::new(DatagramQueueAudit::default()))
                    .clone();
                a.space.store(space, Relaxed);
                if space == 0 {
                    a.full.fetch_add(1, Relaxed);
                }
                match conn.send_datagram(data) {
                    Ok(()) => {
                        a.handoff.fetch_add(1, Relaxed);
                        Ok(())
                    }
                    Err(e) => {
                        a.err.fetch_add(1, Relaxed);
                        Err(e.into())
                    }
                }
            }
        }
    }

    /// Datagram send-queue audit readout for a path (`RWM_DIAG` only):
    /// `(handoff, full, err, space_bytes, tx_frames)`. `None` when the audit
    /// is off or the path has never sent a datagram — which is the off-value
    /// property: no audit, no gauge, rather than a gauge reading zero for two
    /// different reasons.
    ///
    /// `tx_frames` is read live from quinn (`stats().frame_tx.datagram`) —
    /// DATAGRAM frames actually transmitted. See `DatagramQueueAudit` for why
    /// `handoff − tx_frames` is the eviction estimate that does not depend on
    /// the `full` predicate.
    pub fn datagram_queue_stats(&self, path_id: PathId) -> Option<(u64, u64, u64, u64, u64)> {
        use std::sync::atomic::Ordering::Relaxed;
        if !self.dg_audit {
            return None;
        }
        let a = self.dg_stats.get(&path_id)?;
        let tx_frames = self
            .connections
            .get(&path_id)
            .map_or(0, |c| c.stats().frame_tx.datagram);
        Some((
            a.handoff.load(Relaxed),
            a.full.load(Relaxed),
            a.err.load(Relaxed),
            a.space.load(Relaxed),
            tx_frames,
        ))
    }

    /// Receive-side datagram audit for a path (`RWM_DIAG` only):
    /// `(frame_rx, app_read)` — DATAGRAM frames quinn accepted at the packet
    /// layer, and datagrams the app's receive loop read. `frame_rx −
    /// app_read` is what quinn's bounded incoming buffer dropped (oldest
    /// first) plus what is still buffered at the read: a local drop, which
    /// the loss tracker cannot tell from wire loss. `None` when the audit is
    /// off or the path has no receive loop.
    pub fn datagram_rx_audit(&self, path_id: PathId) -> Option<(u64, u64)> {
        if !self.dg_audit {
            return None;
        }
        let read = self
            .dg_rx_read
            .get(&path_id)?
            .load(std::sync::atomic::Ordering::Relaxed);
        let (frame_rx, _) = self.datagram_frame_stats(path_id)?;
        Some((frame_rx, read))
    }

    /// Connect to a peer on a specific path.
    pub async fn connect(&self, path_id: PathId, peer_addr: SocketAddr) -> anyhow::Result<()> {
        let endpoint = self
            .endpoints
            .get(&path_id)
            .ok_or_else(|| anyhow::anyhow!("no endpoint for path {path_id}"))?;

        let connection = endpoint.connect(peer_addr, "raptorpath")?.await?;

        // ADR-0010: perform handshake
        let local_hs = Handshake {
            version: PROTOCOL_VERSION,
            max_block_size: 64 * 1024,
            symbol_size: 1200,
            path_id,
        };
        let _peer_hs = Self::perform_handshake(&connection, &local_hs).await?;

        info!(path_id, %peer_addr, "connected and handshake complete");
        self.connections.insert(path_id, connection);
        Ok(())
    }

    /// Accept an incoming connection on a specific path.
    pub async fn accept(&self, path_id: PathId) -> anyhow::Result<()> {
        let endpoint = self
            .endpoints
            .get(&path_id)
            .ok_or_else(|| anyhow::anyhow!("no endpoint for path {path_id}"))?;

        let incoming = endpoint
            .accept()
            .await
            .ok_or_else(|| anyhow::anyhow!("endpoint closed"))?;
        let connection = incoming.await?;

        // ADR-0010: accept handshake from peer
        let local_hs = Handshake {
            version: PROTOCOL_VERSION,
            max_block_size: 64 * 1024,
            symbol_size: 1200,
            path_id,
        };
        let _peer_hs = Self::accept_handshake(&connection, &local_hs).await?;

        info!(path_id, remote = %connection.remote_address(), "accepted with handshake");
        self.connections.insert(path_id, connection);
        Ok(())
    }

    /// Add a new path at runtime. Binds a new endpoint, connects or accepts,
    /// and returns the connection for receiver spawning.
    pub async fn add_path(
        &self,
        path_id: PathId,
        bind_addr: SocketAddr,
        peer_addr: Option<SocketAddr>,
    ) -> anyhow::Result<quinn::Connection> {
        // Create and bind new endpoint
        let cc = Self::cc_factory_for_path(
            self.cc_passthrough,
            &self.cc_windows,
            &self.cc_stats,
            path_id,
        );
        self.add_endpoint(path_id, bind_addr, cc)?;

        // Connect or accept
        if let Some(peer) = peer_addr {
            self.connect(path_id, peer).await?;
        } else {
            self.accept(path_id).await?;
        }

        let conn = self
            .connections
            .get(&path_id)
            .ok_or_else(|| anyhow::anyhow!("connection not found after setup"))?
            .clone();
        Ok(conn)
    }

    /// Bind `path_id`'s endpoint (the `add_path` half before connect/accept).
    fn add_endpoint(
        &self,
        path_id: PathId,
        bind_addr: SocketAddr,
        cc: Option<Arc<dyn quinn::congestion::ControllerFactory + Send + Sync + 'static>>,
    ) -> anyhow::Result<()> {
        let (endpoint, probe, grant) = if self.is_server {
            let (server_config, _cert) = Self::generate_self_signed_config(cc)?;
            bind_endpoint(path_id, bind_addr, Some(server_config))?
        } else {
            let (mut ep, probe, grant) = bind_endpoint(path_id, bind_addr, None)?;
            ep.set_default_client_config(Self::make_client_config(self.pinned_cert_hash, cc));
            (ep, probe, grant)
        };
        self.endpoints.insert(path_id, endpoint);
        self.rx_probe.insert(path_id, probe);
        self.rcvbuf_grants.insert(path_id, grant);
        Ok(())
    }

    /// Remove a path at runtime.
    pub fn remove_path(&self, path_id: PathId) {
        if let Some((_, conn)) = self.connections.remove(&path_id) {
            conn.close(0u32.into(), b"path removed");
        }
        self.endpoints.remove(&path_id);
        self.rx_probe.remove(&path_id);
        self.rcvbuf_grants.remove(&path_id);
        info!(path_id, "path removed");
    }

    /// Spawn receive loops for a single path, feeding into a channel.
    pub fn spawn_receiver_for_path(
        &self,
        path_id: PathId,
        conn: quinn::Connection,
        tx: mpsc::Sender<(PathId, WireMessage)>,
        ctrl_tx: mpsc::Sender<(PathId, WireMessage)>,
    ) -> Vec<tokio::task::JoinHandle<()>> {
        let mut handles = vec![];

        let conn_uni = conn.clone();
        // `RWM_DIAG` receive-side audit counter (None = audit off: no atomic
        // on the shipped path).
        let rx_read = self.dg_audit.then(|| {
            self.dg_rx_read
                .entry(path_id)
                .or_insert_with(|| Arc::new(std::sync::atomic::AtomicU64::new(0)))
                .clone()
        });
        // Stream-origin control messages go to a dedicated channel: the
        // data channel backs up under symbol floods, and liveness
        // (PathReport/Ping) queued behind it would starve the dead-path
        // check and kill the tunnel under bulk transfers.
        let tx_uni = ctrl_tx;

        // Datagram receiver
        let handle = tokio::spawn(async move {
            loop {
                match conn.read_datagram().await {
                    Ok(data) => match {
                        if let Some(c) = &rx_read {
                            c.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        }
                        WireMessage::deserialize(&data)
                    } {
                        Ok(msg) => {
                            if tx.send((path_id, msg)).await.is_err() {
                                break;
                            }
                        }
                        Err(e) => {
                            warn!(path_id, ?e, "failed to deserialize datagram");
                        }
                    },
                    Err(e) => {
                        error!(path_id, ?e, "datagram receive error");
                        break;
                    }
                }
            }
        });
        handles.push(handle);

        // Uni-stream receiver (for reliable control messages)
        let uni_handle = tokio::spawn(async move {
            loop {
                match conn_uni.accept_uni().await {
                    Ok(mut recv) => {
                        tracing::debug!(path_id, "uni stream accepted");
                        let mut len_buf = [0u8; 4];
                        if let Err(e) = recv.read_exact(&mut len_buf).await {
                            tracing::debug!(path_id, ?e, "uni stream length read failed");
                            continue;
                        }
                        let len = u32::from_be_bytes(len_buf) as usize;
                        if len > 1_000_000 { continue; }
                        let mut data = vec![0u8; len];
                        if recv.read_exact(&mut data).await.is_err() {
                            continue;
                        }
                        match WireMessage::deserialize(&data) {
                            Ok(msg) => {
                                if tx_uni.send((path_id, msg)).await.is_err() {
                                    break;
                                }
                            }
                            Err(e) => {
                                warn!(path_id, ?e, "failed to deserialize uni stream message");
                            }
                        }
                    }
                    Err(e) => {
                        error!(path_id, ?e, "uni stream accept error");
                        break;
                    }
                }
            }
        });
        handles.push(uni_handle);

        handles
    }

    /// Perform handshake on a connection (client side). Returns the peer's handshake.
    async fn perform_handshake(
        conn: &quinn::Connection,
        local: &Handshake,
    ) -> anyhow::Result<Handshake> {
        let (mut send, mut recv) = conn.open_bi().await?;

        let data = local.serialize()?;
        send.write_all(&(data.len() as u32).to_be_bytes()).await?;
        send.write_all(&data).await?;
        send.finish()?;

        let mut len_buf = [0u8; 4];
        recv.read_exact(&mut len_buf).await?;
        let len = u32::from_be_bytes(len_buf) as usize;
        if len > 10_000 {
            anyhow::bail!("handshake too large: {len} bytes");
        }
        let mut buf = vec![0u8; len];
        recv.read_exact(&mut buf).await?;

        let peer = Handshake::deserialize(&buf)?;
        info!(
            local_version = local.version,
            peer_version = peer.version,
            peer_path_id = peer.path_id,
            "handshake complete"
        );
        Ok(peer)
    }

    /// Accept a handshake from a peer (server side).
    async fn accept_handshake(
        conn: &quinn::Connection,
        local: &Handshake,
    ) -> anyhow::Result<Handshake> {
        let (mut send, mut recv) = conn.accept_bi().await?;

        let mut len_buf = [0u8; 4];
        recv.read_exact(&mut len_buf).await?;
        let len = u32::from_be_bytes(len_buf) as usize;
        if len > 10_000 {
            anyhow::bail!("handshake too large: {len} bytes");
        }
        let mut buf = vec![0u8; len];
        recv.read_exact(&mut buf).await?;

        let peer = Handshake::deserialize(&buf)?;

        let data = local.serialize()?;
        send.write_all(&(data.len() as u32).to_be_bytes()).await?;
        send.write_all(&data).await?;
        send.finish()?;

        info!(
            local_version = local.version,
            peer_version = peer.version,
            peer_path_id = peer.path_id,
            "handshake complete (server)"
        );
        Ok(peer)
    }

    /// Send a symbol batch over a path using QUIC datagrams.
    ///
    /// With `RWM_WIRE_COMPACT` (default ON, v5 framing), one-symbol batches
    /// — the window-mode data path, one symbol per datagram — ride the
    /// compact tag+varint frame (~14–16 B vs the 65-B magic+bincode
    /// framing). Multi-symbol (block-mode) batches and everything else keep
    /// the bincode framing.
    pub fn send_symbols(&self, path_id: PathId, batch: SymbolBatch) -> anyhow::Result<()> {
        let conn = self
            .connections
            .get(&path_id)
            .ok_or_else(|| anyhow::anyhow!("no connection on path {path_id}"))?;

        // The two `RWM_CPUPROF` seams of the send path, adjacent rather than
        // nested so their shares add rather than over-count:
        //   `ser`  the wire serialization (compact v5, or bincode)
        //   `hand` the datagram handoff to quinn — not the send syscall, which
        //          happens on quinn's endpoint driver task and is invisible
        //          here by construction (see `net::cpuprof` module docs).
        use crate::net::cpuprof::{timed, Seam};
        if crate::transport::protocol::wire_compact_active() {
            let compact = timed(Seam::Ser, || {
                crate::transport::protocol::serialize_data_compact(&batch)
            });
            if let Some(buf) = compact {
                return timed(Seam::Hand, || self.send_datagram_shaped(path_id, &conn, buf.into()));
            }
        }
        let msg = WireMessage::Data(batch);
        let data = timed(Seam::Ser, || msg.serialize())?;

        timed(Seam::Hand, || self.send_datagram_shaped(path_id, &conn, data.into()))
    }

    /// [`Self::send_symbols`] for ONE symbol without building a
    /// `SymbolBatch` — the window sender's per-datagram path. Sends exactly
    /// what `send_symbols(path_id, SymbolBatch::new(vec![sym.clone()],
    /// send_timestamp_us, seqs, path_id).with_eta(eta_rel_us))` sends, byte
    /// for byte (the compact frame through the same writer; the bincode
    /// fallback through that very call), and fails the same way on a
    /// missing connection. The compact frame is carved from the caller's
    /// `arena` (see `protocol::serialize_symbol_compact_in`).
    pub fn send_symbol(
        &self,
        path_id: PathId,
        sym: &crate::fec::WireSymbol,
        send_timestamp_us: u64,
        seqs: (u64, u64),
        eta_rel_us: u64,
        arena: &mut bytes::BytesMut,
    ) -> anyhow::Result<()> {
        if !crate::transport::protocol::wire_compact_active() {
            let batch = SymbolBatch::new(vec![sym.clone()], send_timestamp_us, seqs, path_id)
                .with_eta(eta_rel_us);
            return self.send_symbols(path_id, batch);
        }
        let conn = self
            .connections
            .get(&path_id)
            .ok_or_else(|| anyhow::anyhow!("no connection on path {path_id}"))?;
        use crate::net::cpuprof::{timed, Seam};
        let buf = timed(Seam::Ser, || {
            crate::transport::protocol::serialize_symbol_compact_in(
                arena,
                sym,
                send_timestamp_us,
                seqs,
                path_id,
                eta_rel_us,
            )
        });
        timed(Seam::Hand, || self.send_datagram_shaped(path_id, &conn, buf))
    }

    /// Send a control message as a datagram (best-effort, low latency).
    pub fn send_control_datagram(&self, path_id: PathId, msg: ControlMessage) -> anyhow::Result<()> {
        let conn = self
            .connections
            .get(&path_id)
            .ok_or_else(|| anyhow::anyhow!("no connection on path {path_id}"))?;
        let wire = WireMessage::Control(msg);
        let data = wire.serialize()?;
        self.send_datagram_shaped(path_id, &conn, data.into())
    }

    /// Send a control message over a path's reliable stream.
    pub async fn send_control(
        &self,
        path_id: PathId,
        msg: ControlMessage,
    ) -> anyhow::Result<()> {
        let conn = self
            .connections
            .get(&path_id)
            .ok_or_else(|| anyhow::anyhow!("no connection on path {path_id}"))?;

        let mut send = conn.open_uni().await?;
        let wire = WireMessage::Control(msg);
        let data = wire.serialize()?;

        send.write_all(&(data.len() as u32).to_be_bytes()).await?;
        send.write_all(&data).await?;
        send.finish()?;
        Ok(())
    }

    /// Query the max datagram size for a path (PMTU-based).
    pub fn max_datagram_size(&self, path_id: PathId) -> Option<usize> {
        self.connections
            .get(&path_id)
            .and_then(|conn| conn.max_datagram_size())
    }

    /// Receive datagrams from a path.
    pub async fn recv_datagram(&self, path_id: PathId) -> anyhow::Result<WireMessage> {
        let conn = self
            .connections
            .get(&path_id)
            .ok_or_else(|| anyhow::anyhow!("no connection on path {path_id}"))?;

        let data = conn.read_datagram().await?;
        let msg = WireMessage::deserialize(&data)?;
        Ok(msg)
    }

    /// Spawn receive loops for all paths, feeding into a channel.
    pub fn spawn_receivers(
        &self,
        tx: mpsc::Sender<(PathId, WireMessage)>,
        ctrl_tx: mpsc::Sender<(PathId, WireMessage)>,
    ) -> Vec<tokio::task::JoinHandle<()>> {
        let mut handles = vec![];

        for entry in self.connections.iter() {
            let path_id = *entry.key();
            let conn = entry.value().clone();
            handles.extend(self.spawn_receiver_for_path(
                path_id,
                conn,
                tx.clone(),
                ctrl_tx.clone(),
            ));
        }

        handles
    }

    /// Apply the symbol-datagram MTU floor to a quinn transport config
    /// (ADR-0055).
    ///
    /// The wedge: every wire symbol rides one QUIC datagram of ~1261–1275
    /// bytes (1200-byte symbol + repair header + bincode/batch framing).
    /// quinn's defaults are `initial_mtu = min_mtu = 1200`; PMTUD raises the
    /// path MTU to ~1452 right after the handshake, which is the only reason
    /// those datagrams are sendable at all. quinn also runs an MTU
    /// black-hole detector: a burst of lost large packets (GE loss looks
    /// exactly like an MTU black hole) resets `current_mtu` to `min_mtu`
    /// (1200) and pauses discovery for `black_hole_cooldown` (default 60 s).
    /// During that window `max_datagram_size` ≈ 1170 < every symbol
    /// datagram, so every data send — source, repair, and every targeted
    /// retransmit of the frontier blocker — fails at the sender with
    /// `SendDatagramError::TooLarge`, while small control datagrams (acks)
    /// still flow and keep the wire RTT fresh. The transfer freezes for
    /// exactly the cooldown. It is below the CC layer, so every CC arm hits it.
    ///
    /// The fix: the engine structurally requires ~1275-byte datagrams (a
    /// symbol is never fragmented), so declare that floor to quinn:
    /// `min_mtu = initial_mtu = MTU_FLOOR`. A (possibly spurious)
    /// black-hole reset then lands at the floor and symbol sends keep
    /// working; PMTUD and the black-hole detector otherwise stay active.
    /// A path that truly cannot carry MTU_FLOOR-byte UDP payloads could
    /// never carry a symbol anyway; it now fails loudly as persistent
    /// large-packet loss instead of a silent send blackout.
    ///
    /// `RWM_MTU_FLOOR` overrides (A/B instrument): `0` restores stock quinn
    /// defaults (the wedge-prone control arm), any other value sets the
    /// floor explicitly.
    fn apply_mtu_floor(transport: &mut quinn::TransportConfig) {
        // A max-size repair-symbol batch serializes to 1279 datagram bytes
        // (1200 symbol + 14 repair header + 65 magic/bincode-fixint batch
        // framing — measured by `mtu_floor_covers_symbol_batch`), + ~33
        // QUIC 1-RTT overhead (short header + CID + PN + AEAD tag +
        // DATAGRAM frame header) = ~1312 minimum UDP payload; 1350 leaves
        // margin for CID/PN-length variation.
        const MTU_FLOOR: u16 = 1350;
        let floor: u16 = crate::gates::get()
            .mtu_floor_raw
            .as_deref()
            .and_then(|s| s.parse().ok())
            .unwrap_or(MTU_FLOOR);
        if floor == 0 {
            info!("MTU floor OFF (RWM_MTU_FLOOR=0): stock quinn MTUD — black-hole reset lands at 1200 < symbol datagram (wedge-reproduction arm)");
            return; // stock quinn MTU behavior (wedge-reproduction arm)
        }
        info!(floor, "MTU floor: min_mtu=initial_mtu — quinn black-hole reset keeps symbol datagrams sendable (ADR-0055)");
        transport.initial_mtu(floor);
        transport.min_mtu(floor);
        // Mechanism-liveness echo (docs/measurement-discipline.md rule 1):
        // the compact-framing gate, resolved once.
        if crate::transport::protocol::wire_compact_active() {
            info!(
                "compact DATA framing ACTIVE (RWM_WIRE_COMPACT v5: one-symbol \
                 datagrams ride the tag+varint frame, ~14-16 B vs 65-B legacy \
                 framing; receive support unconditional; datagrams SHRINK — \
                 no MTU-floor interaction)"
            );
        }
    }

    fn generate_self_signed_config(
        cc: Option<Arc<dyn quinn::congestion::ControllerFactory + Send + Sync + 'static>>,
    ) -> anyhow::Result<(ServerConfig, Vec<CertificateDer<'static>>)> {
        let cert = rcgen::generate_simple_self_signed(vec!["raptorpath".into()])?;
        let cert_der = CertificateDer::from(cert.cert);
        let key_der = PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der());

        let mut server_config = ServerConfig::with_single_cert(
            vec![cert_der.clone()],
            key_der.into(),
        )?;

        // Enable datagrams
        let transport = Arc::get_mut(&mut server_config.transport).unwrap();
        transport.max_concurrent_bidi_streams(100u32.into());
        transport.max_concurrent_uni_streams(100u32.into());
        transport.datagram_receive_buffer_size(Some(4 * 1024 * 1024));
        transport.datagram_send_buffer_size(4 * 1024 * 1024);
        Self::apply_mtu_floor(transport);
        if let Some(cc) = cc {
            transport.congestion_controller_factory(cc);
        }

        Ok((server_config, vec![cert_der]))
    }

    /// Build a client config with either pinned cert verification or
    /// insecure mode (skip verification) for dev/testing.
    fn make_client_config(
        pinned_hash: Option<[u8; 32]>,
        cc: Option<Arc<dyn quinn::congestion::ControllerFactory + Send + Sync + 'static>>,
    ) -> ClientConfig {
        let verifier: Arc<dyn rustls::client::danger::ServerCertVerifier> = match pinned_hash {
            Some(hash) => Arc::new(PinnedCertVerifier { expected_hash: hash }),
            None => {
                // No behaviour change: dev/test mode stays available, but it
                // must never be silent. Once per process.
                static WARNED: std::sync::Once = std::sync::Once::new();
                WARNED.call_once(|| {
                    tracing::warn!(
                        "TLS server certificate verification is DISABLED: no \
                         certificate pin was given (--pin-cert), so the peer is \
                         not authenticated. Pass --pin-cert outside dev/test."
                    );
                });
                Arc::new(SkipCertVerification)
            }
        };

        let crypto = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(verifier)
            .with_no_client_auth();

        let mut config = ClientConfig::new(Arc::new(
            quinn::crypto::rustls::QuicClientConfig::try_from(crypto)
                .expect("rustls config should be valid"),
        ));

        let mut transport = quinn::TransportConfig::default();
        transport.datagram_receive_buffer_size(Some(4 * 1024 * 1024));
        transport.datagram_send_buffer_size(4 * 1024 * 1024);
        Self::apply_mtu_floor(&mut transport);
        if let Some(cc) = cc {
            transport.congestion_controller_factory(cc);
        }
        config.transport_config(Arc::new(transport));
        config
    }
}

/// Compute SHA-256 fingerprint of a DER-encoded certificate.
fn sha256_fingerprint(cert: &CertificateDer<'_>) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(cert.as_ref());
    hasher.finalize().into()
}

/// Load a pinned certificate from a DER or PEM file and return its SHA-256 hash.
fn load_pinned_cert_hash(path: &Path) -> anyhow::Result<[u8; 32]> {
    let data = std::fs::read(path)
        .map_err(|e| anyhow::anyhow!("failed to read pinned cert '{}': {e}", path.display()))?;

    // Try PEM first, fall back to DER
    let cert_der = if data.starts_with(b"-----BEGIN") {
        let pem = pem::parse(&data)
            .map_err(|e| anyhow::anyhow!("failed to parse PEM cert '{}': {e}", path.display()))?;
        CertificateDer::from(pem.into_contents())
    } else {
        CertificateDer::from(data)
    };

    Ok(sha256_fingerprint(&cert_der))
}

/// Certificate verifier that pins to a specific certificate's SHA-256 fingerprint.
#[derive(Debug)]
struct PinnedCertVerifier {
    expected_hash: [u8; 32],
}

impl rustls::client::danger::ServerCertVerifier for PinnedCertVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        let actual_hash = sha256_fingerprint(end_entity);
        if actual_hash == self.expected_hash {
            Ok(rustls::client::danger::ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::General(format!(
                "certificate fingerprint mismatch: expected {}, got {}",
                hex::encode(self.expected_hash),
                hex::encode(actual_hash),
            )))
        }
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::RSA_PKCS1_SHA384,
            rustls::SignatureScheme::RSA_PKCS1_SHA512,
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
            rustls::SignatureScheme::ED25519,
        ]
    }
}

/// Skip certificate verification (for self-signed certs in testing/dev).
#[derive(Debug)]
struct SkipCertVerification;

impl rustls::client::danger::ServerCertVerifier for SkipCertVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::RSA_PKCS1_SHA384,
            rustls::SignatureScheme::RSA_PKCS1_SHA512,
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
            rustls::SignatureScheme::ED25519,
        ]
    }
}

#[cfg(test)]
mod passthrough_cc_tests {
    use super::*;
    use quinn::congestion::{Controller, ControllerFactory};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Instant;

    fn build(window: &Arc<AtomicU64>, mtu: u16) -> Box<dyn Controller> {
        let f = Arc::new(PassthroughFactory {
            window: window.clone(),
            stats: Arc::new(PassthroughCcStats::default()),
        });
        f.build(Instant::now(), mtu)
    }

    /// The shim's window() follows the engine-owned atomic: what our engine
    /// writes is the substrate congestion window.
    #[test]
    fn window_follows_the_atomic() {
        let w = Arc::new(AtomicU64::new(PASSTHROUGH_INITIAL_WINDOW));
        let c = build(&w, 1200);
        assert_eq!(c.window(), PASSTHROUGH_INITIAL_WINDOW);
        w.store(37_500, Ordering::Relaxed); // Copa cwnd 30 sym x 1250 B
        assert_eq!(c.window(), 37_500);
        w.store(1_000_000, Ordering::Relaxed);
        assert_eq!(c.window(), 1_000_000);
    }

    /// The handshake is never starved: the initial window is generous (the
    /// atomic starts at PASSTHROUGH_INITIAL_WINDOW, well above quinn's stock
    /// RFC-9002 initial ~14 720 B) and a zero/garbage write floors at two
    /// datagrams instead of wedging the connection.
    #[test]
    fn handshake_not_starved_and_zero_write_floors() {
        let w = Arc::new(AtomicU64::new(PASSTHROUGH_INITIAL_WINDOW));
        let c = build(&w, 1500);
        assert!(c.initial_window() >= 64 * 1024);
        assert!(c.window() >= 64 * 1024, "pre-feed window must cover the handshake");
        w.store(0, Ordering::Relaxed);
        assert_eq!(c.window(), 2 * 1500, "zero write floors at 2 MTUs, never 0");
    }

    /// clone_box (quinn clones controllers for path state) keeps sharing the
    /// same engine-owned atomic.
    #[test]
    fn clone_box_shares_the_atomic() {
        let w = Arc::new(AtomicU64::new(50_000));
        let c = build(&w, 1200);
        let c2 = c.clone_box();
        w.store(80_000, Ordering::Relaxed);
        assert_eq!(c.window(), 80_000);
        assert_eq!(c2.window(), 80_000);
    }

    /// Congestion events are recorded, never acted on: window unchanged.
    #[test]
    fn congestion_events_are_recorded_noops() {
        let w = Arc::new(AtomicU64::new(100_000));
        let stats = Arc::new(PassthroughCcStats::default());
        let f = Arc::new(PassthroughFactory { window: w.clone(), stats: stats.clone() });
        let mut c = f.build(Instant::now(), 1200);
        let now = Instant::now();
        c.on_congestion_event(now, now, false, 3_600);
        c.on_congestion_event(now, now, true, 1_200);
        assert_eq!(c.window(), 100_000, "loss must not move the engine-owned window");
        assert_eq!(stats.congestion_events.load(Ordering::Relaxed), 2);
        assert_eq!(stats.lost_bytes.load(Ordering::Relaxed), 4_800);
        assert_eq!(stats.persistent_congestion.load(Ordering::Relaxed), 1);
    }
}

#[cfg(test)]
mod datagram_queue_audit_tests {
    use super::*;

    /// **The off-value property** for the datagram send-queue audit.
    ///
    /// The audit costs one connection-lock take per datagram, so it must not
    /// exist on the shipped path. `dg_audit` resolves `RWM_DIAG` once at
    /// construction, and with it off `datagram_queue_stats` must return
    /// `None` for every path — not `Some((0,0,0,0,0))`, which would be
    /// indistinguishable from "the audit ran and saw nothing".
    ///
    /// **Written to be correct in both process conditions.** A test that
    /// mutated `RWM_DIAG` would race every other test in the process (and is
    /// `unsafe` in edition 2024), so this reads the ambient value and asserts
    /// the branch that value selects. Run it in a process with `RWM_DIAG`
    /// unset and again with `RWM_DIAG=1` and both arms are covered — which is
    /// the multi-process discipline this repo already applies to env gates.
    #[tokio::test]
    async fn datagram_queue_audit_follows_rwm_diag_and_is_absent_when_off() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let addr: std::net::SocketAddr = "127.0.0.1:0".parse().unwrap();
        let t = QuicTransport::new(&[addr], false, None)
            .await
            .expect("client endpoint binds on loopback");

        let diag_on = crate::config::env_flag("RWM_DIAG", false);
        assert_eq!(
            t.dg_audit, diag_on,
            "dg_audit must be RWM_DIAG resolved at construction"
        );

        // No path has sent a datagram, so the readout is `None` either way —
        // the two reasons are distinguished by `dg_audit`, never by a zero.
        assert!(
            t.datagram_queue_stats(0).is_none(),
            "no handoff has happened: the gauge must be absent, not zero"
        );
        assert!(
            t.datagram_rx_audit(0).is_none(),
            "no receive loop exists: the rx audit must be absent, not zero"
        );

        if !diag_on {
            // The stronger off claim: the map itself is never touched, so the
            // seam takes no lock and allocates no per-path audit record.
            assert!(
                t.dg_stats.is_empty(),
                "the audit must not allocate when RWM_DIAG is off"
            );
        }
    }

    /// The eviction predicate's semantics, asserted against quinn's actual
    /// source so the bound in `DatagramQueueAudit`'s docs is checked rather
    /// than merely claimed.
    ///
    /// quinn-proto's `Datagrams::send(_, drop = true)` pops while
    /// `outgoing_total > buffer_size`, and `send_buffer_space()` is
    /// `buffer_size.saturating_sub(outgoing_total)`. Therefore
    /// `space == 0  <=>  outgoing_total >= buffer_size`, which contains the
    /// eviction condition `outgoing_total > buffer_size` and exceeds it only
    /// on the exact tie. This test states that containment as arithmetic.
    #[test]
    fn the_full_predicate_contains_the_eviction_condition_and_differs_only_at_the_tie() {
        const SIZE: usize = 4 * 1024 * 1024;
        let space = |total: usize| SIZE.saturating_sub(total);
        let evicts = |total: usize| total > SIZE;
        let full = |total: usize| space(total) == 0;

        for total in [0, 1, SIZE / 2, SIZE - 1, SIZE, SIZE + 1, SIZE + 1_200, SIZE * 2] {
            assert!(
                !evicts(total) || full(total),
                "every evicting state must be counted by `full` (total={total})"
            );
        }
        // The one over-count, named explicitly rather than left implicit.
        assert!(full(SIZE) && !evicts(SIZE), "the tie is the only over-count");
        assert!(!full(SIZE - 1), "a queue with room must not be counted");
    }
}

#[cfg(all(test, target_os = "linux"))]
mod rcvbuf_endpoint_tests {
    use super::*;

    /// Every endpoint socket the transport binds — server and client role,
    /// at construction and through `add_path`'s bind — carries the
    /// receive-buffer request: the live `getsockopt(SO_RCVBUF)` on the
    /// endpoint's own socket reaches `2 × min(req, rmem_max)` (Linux doubles
    /// the request; as root `2 × req`). A socket bound the way quinn's
    /// `Endpoint::server`/`client` bind it reads `rmem_default` (212 992)
    /// and fails here.
    #[tokio::test]
    async fn every_endpoint_socket_reads_back_the_rcvbuf_floor() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let floor = super::super::rcvbuf::tests::linux_floor(rcvbuf::RCVBUF_REQUEST);
        let any: SocketAddr = "127.0.0.1:0".parse().unwrap();
        for is_server in [true, false] {
            let t = QuicTransport::new(&[any, any], is_server, None)
                .await
                .expect("endpoints bind on loopback");
            t.add_endpoint(7, any, None).expect("add_path's bind");
            for pid in [0, 1, 7] {
                let live = t.rx_socket_rcvbuf(pid).expect("probe socket per path");
                let g = t.rcvbuf_grant(pid).expect("grant recorded per path");
                assert_eq!(g.requested, rcvbuf::RCVBUF_REQUEST);
                assert_eq!(g.granted, live, "the recorded grant is the live read-back");
                assert!(
                    live >= floor,
                    "server={is_server} p{pid}: SO_RCVBUF {live} < floor {floor}"
                );
                assert_eq!(t.rx_socket_drops(pid), Some(0), "fresh socket, SO_MEMINFO drops");
            }
            t.remove_path(7);
            assert!(t.rx_socket_rcvbuf(7).is_none(), "the probe leaves with the path");
        }
    }
}
