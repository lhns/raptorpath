//! QUIC transport implementation using quinn.
//!
//! Each path gets its own QUIC connection. We use:
//! - DATAGRAM frames for symbol data (unreliable, low overhead)
//! - A bidirectional stream for control messages (reliable)
//!
//! TLS modes:
//! - Default: self-signed cert, skip verification (dev/testing)
//! - Pinned: verify server cert matches a pinned DER/PEM file (production)

use super::io_owner::{
    self, InboundBatch, IoCmd, Lane, PathIo, PathView, TxBatch,
};
use super::l0_netem::L0Netem;
use super::rcvbuf::{self, RcvbufGrant};
use super::protocol::{ControlMessage, SymbolBatch, WireMessage};
use crate::scheduler::PathId;
use dashmap::DashMap;
use quinn::{ClientConfig, ServerConfig};
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
use sha2::{Digest, Sha256};
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{info, warn};

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
/// Threading Q1: every path is served by its I/O owner
/// (`transport/io_owner.rs`), the only code that calls quinn for that path.
/// This struct holds, per path, the owner's channel and its published view —
/// never a `quinn::Connection` — plus the non-quinn per-path state (the
/// pass-through CC windows, the receive-buffer probe sockets).
pub struct QuicTransport {
    /// Per path: the owner's channel and its published view.
    paths: DashMap<PathId, PathIo>,
    /// Bumped on every path add/remove: producers' cached handles revalidate
    /// against it (`TxBatch`), so no map is read per datagram.
    epoch: std::sync::atomic::AtomicU64,
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
    /// `RWM_DIAG`: run the datagram send-queue audit in the owner. Resolved
    /// once, here, so the shipped default pays neither the extra
    /// connection-lock take nor the atomics.
    dg_audit: bool,
    /// Per path: a clone of the endpoint's UDP socket (same kernel socket,
    /// second fd), kept to read the socket's live `SO_RCVBUF` and its
    /// kernel drop count (`SO_MEMINFO`) — quinn does not expose the fd.
    /// Removed with the path so the port is released with the endpoint.
    rx_probe: DashMap<PathId, std::net::UdpSocket>,
    /// Per path: the `SO_RCVBUF` request and the read-back grant at bind
    /// (transport/rcvbuf.rs; echoed once as `[RCVBUF]`).
    rcvbuf_grants: DashMap<PathId, RcvbufGrant>,
    /// The main runtime, captured at construction: every owner is a task
    /// on it.
    rt: tokio::runtime::Handle,
    /// The driver-routing probe (`RWM_RTOBS`, or a test): wrap each path's
    /// congestion controller to record which thread quinn's driver polls on.
    route_probe: bool,
    /// The lock-wait gauge's clock reads (`RWM_RTOBS`).
    gauge: bool,
}

impl QuicTransport {
    /// Create a new transport with endpoints bound to the given addresses.
    ///
    /// `pin_cert_path`: optional path to a DER or PEM certificate file.
    /// When provided, the client will verify that the server's certificate
    /// matches this pinned cert (SHA-256 fingerprint comparison).
    ///
    /// Every path's owner is a task on the current (main) runtime.
    pub async fn new(
        bind_addrs: &[SocketAddr],
        is_server: bool,
        pin_cert_path: Option<&Path>,
    ) -> anyhow::Result<Self> {
        let g = crate::gates::get();
        Self::new_probed(bind_addrs, is_server, pin_cert_path, g.rtobs).await
    }

    /// [`Self::new`] with an explicit routing-probe switch (a test arms it
    /// without `RWM_RTOBS`).
    pub async fn new_probed(
        bind_addrs: &[SocketAddr],
        is_server: bool,
        pin_cert_path: Option<&Path>,
        route_probe: bool,
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
        let t = Self {
            paths: DashMap::new(),
            epoch: std::sync::atomic::AtomicU64::new(0),
            is_server,
            pinned_cert_hash,
            l0_netem: L0Netem::from_env(),
            cc_passthrough,
            cc_windows: DashMap::new(),
            cc_stats: DashMap::new(),
            // Resolved once, at construction: the audit's per-datagram cost
            // (one connection-lock take for `datagram_send_buffer_space`)
            // must not exist on the shipped path. `RWM_DIAG` is already in
            // `RWM_FORWARD` and the `[GATES]` echo, so this adds no name to
            // the gate surface.
            dg_audit: crate::gates::get().diag,
            rx_probe: DashMap::new(),
            rcvbuf_grants: DashMap::new(),
            rt: tokio::runtime::Handle::current(),
            route_probe,
            gauge: crate::gates::get().rtobs,
        };
        for (i, addr) in bind_addrs.iter().enumerate() {
            t.spawn_path(i as PathId, *addr).await?;
        }
        Ok(t)
    }

    /// Spawn `path_id`'s owner on the main runtime and wait until it has
    /// bound the path's endpoint there.
    async fn spawn_path(&self, path_id: PathId, bind_addr: SocketAddr) -> anyhow::Result<()> {
        let side = if self.is_server { "server" } else { "client" };
        let view = PathView::new(path_id, side, "main".to_string());
        let mut cc = Self::cc_factory_for_path(
            self.cc_passthrough,
            &self.cc_windows,
            &self.cc_stats,
            path_id,
        );
        if self.route_probe {
            cc = cc.map(|inner| {
                Arc::new(io_owner::RouteProbeFactory { inner, view: view.clone() })
                    as Arc<dyn quinn::congestion::ControllerFactory + Send + Sync + 'static>
            });
        }
        let (server_config, client_config) = if self.is_server {
            let (server_config, cert_der) = Self::generate_self_signed_config(cc)?;
            // Log the server cert fingerprint so the user can pin it on the client
            let fingerprint = sha256_fingerprint(&cert_der[0]);
            info!(addr = %bind_addr, path_id, fingerprint = %hex::encode(fingerprint),
                "server endpoint bound — use this fingerprint for --pin-cert");
            (Some(server_config), None)
        } else {
            info!(addr = %bind_addr, path_id, "client endpoint bound");
            (None, Some(Self::make_client_config(self.pinned_cert_hash, cc)))
        };
        let (tx, cmd_rx) = mpsc::channel(io_owner::IO_CHANNEL_DEPTH);
        let (bound_tx, bound_rx) = tokio::sync::oneshot::channel();
        let args = io_owner::OwnerArgs {
            path: path_id,
            bind: bind_addr,
            server_config,
            client_config,
            view: view.clone(),
            cmd_rx,
            bound: Some(bound_tx),
            shim: self.l0_netem.clone(),
            is_server: self.is_server,
            dg_audit: self.dg_audit,
            gauge: self.gauge,
            deep: crate::task_obs::deep(),
        };
        let side = if self.is_server { "server" } else { "client" };
        self.rt
            .spawn(crate::task_obs::timed(format!("owner-{side}-p{path_id}"), io_owner::run_owner(args)));
        let (probe, grant) = bound_rx
            .await
            .map_err(|_| anyhow::anyhow!("path {path_id}: the I/O owner ended before binding"))??;
        self.paths.insert(path_id, PathIo { tx, view });
        self.rx_probe.insert(path_id, probe);
        self.rcvbuf_grants.insert(path_id, grant);
        self.epoch.fetch_add(1, std::sync::atomic::Ordering::Release);
        Ok(())
    }

    /// The path's owner handle (a cold lookup: producers cache it).
    fn io(&self, path_id: PathId) -> Option<PathIo> {
        self.paths.get(&path_id).map(|p| p.value().clone())
    }

    /// The path's published view.
    fn view(&self, path_id: PathId) -> Option<Arc<PathView>> {
        self.paths.get(&path_id).map(|p| p.value().view.clone())
    }

    /// The `SO_RCVBUF` request and the kernel's grant for `path_id`'s
    /// endpoint socket, read back at bind.
    pub fn rcvbuf_grant(&self, path_id: PathId) -> Option<RcvbufGrant> {
        self.rcvbuf_grants.get(&path_id).map(|g| *g)
    }

    /// `path_id`'s endpoint socket's local address (from the probe clone).
    pub fn local_addr(&self, path_id: PathId) -> Option<SocketAddr> {
        self.rx_probe.get(&path_id)?.local_addr().ok()
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
    ///
    /// Threading Q1: read from the owner's published view (refreshed every
    /// owner poll under `RWM_COPA_WIRE`, every `VIEW_REFRESH` otherwise).
    pub fn wire_rtt(&self, path_id: PathId) -> Option<std::time::Duration> {
        self.view(path_id)?.rtt()
    }

    /// Quinn-level DATAGRAM frame counters for `path_id`:
    /// `(datagram_frames_rx, datagram_frames_tx)` from `Connection::stats()`
    /// (the owner's published snapshot, ≤ `VIEW_REFRESH` old).
    ///
    /// Wedge forensics: `frame_rx.datagram` counts every DATAGRAM frame quinn
    /// accepted at the packet layer — before the app's read and before
    /// quinn's bounded incoming datagram buffer (which silently drops the
    /// oldest buffered datagram on overflow). If this counter advances while
    /// the app-level receive loop sees nothing, arriving datagrams are being
    /// destroyed between quinn's packet layer and the application (buffer
    /// overflow), not lost on the wire.
    pub fn datagram_frame_stats(&self, path_id: PathId) -> Option<(u64, u64)> {
        self.view(path_id)?.frame_stats()
    }

    /// Quinn substrate congestion gauge for `path_id` (a diagnosis
    /// instrument — read only at the RWM_DIAG print, never gates anything):
    /// `(cwnd_bytes, congestion_events, lost_packets, sent_packets)` from
    /// `Connection::stats().path` (the owner's published snapshot). Under the
    /// shipped BBR default the cwnd IS 2 × quinn's internal BtlBŵ × RTprop,
    /// so a cwnd many multiples of the true BDP·MTU is direct in-vivo
    /// evidence of the max-filter over-read.
    pub fn quinn_path_stats(&self, path_id: PathId) -> Option<(u64, u64, u64, u64)> {
        self.view(path_id)?.path_stats()
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

    /// Datagram send-queue audit readout for a path (`RWM_DIAG` only):
    /// `(handoff, full, err, space_bytes, tx_frames)`. `None` when the audit
    /// is off or the path has never sent a datagram — which is the off-value
    /// property: no audit, no gauge, rather than a gauge reading zero for two
    /// different reasons.
    ///
    /// `tx_frames` is quinn's `stats().frame_tx.datagram` from the owner's
    /// snapshot — DATAGRAM frames actually transmitted. See
    /// `io_owner::DatagramQueueAudit` for why `handoff − tx_frames` is the
    /// eviction estimate that does not depend on the `full` predicate.
    pub fn datagram_queue_stats(&self, path_id: PathId) -> Option<(u64, u64, u64, u64, u64)> {
        use std::sync::atomic::Ordering::Relaxed;
        if !self.dg_audit {
            return None;
        }
        let v = self.view(path_id)?;
        let a = &v.audit;
        let (handoff, err) = (a.handoff.load(Relaxed), a.err.load(Relaxed));
        if handoff + err == 0 {
            return None;
        }
        let tx_frames = v.frame_stats().map_or(0, |(_, tx)| tx);
        Some((handoff, a.full.load(Relaxed), err, a.space.load(Relaxed), tx_frames))
    }

    /// Receive-side datagram audit for a path (`RWM_DIAG` only):
    /// `(frame_rx, app_read)` — DATAGRAM frames quinn accepted at the packet
    /// layer, and datagrams the owner's reader read. `frame_rx − app_read`
    /// is what quinn's bounded incoming buffer dropped (oldest first) plus
    /// what is still buffered at the read: a local drop, which the loss
    /// tracker cannot tell from wire loss. `None` when the audit is off or
    /// the path's readers were never started.
    pub fn datagram_rx_audit(&self, path_id: PathId) -> Option<(u64, u64)> {
        if !self.dg_audit {
            return None;
        }
        let v = self.view(path_id)?;
        if !v.readers_started.load(std::sync::atomic::Ordering::Relaxed) {
            return None;
        }
        let read = v.app_read.load(std::sync::atomic::Ordering::Relaxed);
        let (frame_rx, _) = v.frame_stats()?;
        Some((frame_rx, read))
    }

    /// Connect to a peer on a specific path (in the path's owner).
    pub async fn connect(&self, path_id: PathId, peer_addr: SocketAddr) -> anyhow::Result<()> {
        let io = self
            .io(path_id)
            .ok_or_else(|| anyhow::anyhow!("no endpoint for path {path_id}"))?;
        let (reply, rx) = tokio::sync::oneshot::channel();
        io.tx
            .send(IoCmd::Connect { peer: peer_addr, reply })
            .await
            .map_err(|_| anyhow::anyhow!("path {path_id}: the I/O owner is gone"))?;
        rx.await.map_err(|_| anyhow::anyhow!("path {path_id}: the I/O owner ended during connect"))?
    }

    /// Accept an incoming connection on a specific path (in the path's
    /// owner).
    pub async fn accept(&self, path_id: PathId) -> anyhow::Result<()> {
        let io = self
            .io(path_id)
            .ok_or_else(|| anyhow::anyhow!("no endpoint for path {path_id}"))?;
        let (reply, rx) = tokio::sync::oneshot::channel();
        io.tx
            .send(IoCmd::Accept { reply })
            .await
            .map_err(|_| anyhow::anyhow!("path {path_id}: the I/O owner is gone"))?;
        rx.await.map_err(|_| anyhow::anyhow!("path {path_id}: the I/O owner ended during accept"))?
    }

    /// Add a new path at runtime: spawn its owner (which binds the endpoint
    /// on its own runtime), then connect or accept there.
    pub async fn add_path(
        &self,
        path_id: PathId,
        bind_addr: SocketAddr,
        peer_addr: Option<SocketAddr>,
    ) -> anyhow::Result<()> {
        self.spawn_path(path_id, bind_addr).await?;
        if let Some(peer) = peer_addr {
            self.connect(path_id, peer).await
        } else {
            self.accept(path_id).await
        }
    }

    /// Remove a path at runtime: its owner closes the connection and ends.
    pub fn remove_path(&self, path_id: PathId) {
        if let Some((_, io)) = self.paths.remove(&path_id) {
            if let Err(mpsc::error::TrySendError::Full(cmd)) = io.tx.try_send(IoCmd::Close) {
                let tx = io.tx.clone();
                tokio::spawn(async move {
                    let _ = tx.send(cmd).await;
                });
            }
        }
        self.rx_probe.remove(&path_id);
        self.rcvbuf_grants.remove(&path_id);
        self.epoch.fetch_add(1, std::sync::atomic::Ordering::Release);
        info!(path_id, "path removed");
    }

    /// Start every path's readers: the owner forwards inbound datagrams to
    /// `tx` (one batch per owner poll), the TX-direction control datagrams
    /// among them (threading Q2: WindowAck / Ack / PathReport / Ping —
    /// `net::control_msg::is_tx_control`) to the window sender's input
    /// `sender_tx` instead (one batch per poll too), and uni-stream control
    /// messages to `ctrl_tx`.
    pub async fn start_readers(
        &self,
        tx: mpsc::Sender<InboundBatch>,
        ctrl_tx: mpsc::Sender<(PathId, WireMessage)>,
        sender_tx: mpsc::Sender<InboundBatch>,
    ) {
        let ids: Vec<PathId> = self.paths.iter().map(|e| *e.key()).collect();
        for path_id in ids {
            self.start_readers_for_path(path_id, tx.clone(), ctrl_tx.clone(), sender_tx.clone()).await;
        }
    }

    /// Start one path's readers (see [`Self::start_readers`]). Stream-origin
    /// control messages go to their own channel: the data channel backs up
    /// under symbol floods, and liveness (PathReport/Ping) queued behind it
    /// would starve the dead-path check and kill the tunnel under bulk
    /// transfers.
    pub async fn start_readers_for_path(
        &self,
        path_id: PathId,
        tx: mpsc::Sender<InboundBatch>,
        ctrl_tx: mpsc::Sender<(PathId, WireMessage)>,
        sender_tx: mpsc::Sender<InboundBatch>,
    ) {
        if let Some(io) = self.io(path_id) {
            let _ = io.tx.send(IoCmd::StartReaders { msg_tx: tx, ctrl_tx, sender_tx }).await;
        }
    }

    /// Stage one serialized datagram for `path_id` in the producer's batch.
    fn stage(&self, out: &mut TxBatch, path_id: PathId, lane: Lane, b: bytes::Bytes) -> anyhow::Result<()> {
        let epoch = self.epoch.load(std::sync::atomic::Ordering::Acquire);
        out.stage(&|p| self.io(p), epoch, path_id, lane, b)
    }

    /// Hand the producer's staged batches to their owners (one `IoCmd` per
    /// path and lane). Call once per producer loop iteration, before the
    /// producer waits.
    pub async fn flush(&self, out: &mut TxBatch) {
        out.flush().await
    }

    /// Send a symbol batch over a path using QUIC datagrams (staged in `out`).
    ///
    /// With `RWM_WIRE_COMPACT` (default ON, v5 framing), one-symbol batches
    /// — the window-mode data path, one symbol per datagram — ride the
    /// compact tag+varint frame (~14–16 B vs the 65-B magic+bincode
    /// framing). Multi-symbol (block-mode) batches and everything else keep
    /// the bincode framing.
    pub fn send_symbols(&self, out: &mut TxBatch, path_id: PathId, batch: SymbolBatch) -> anyhow::Result<()> {
        // The two `RWM_CPUPROF` seams of the send path, adjacent rather than
        // nested so their shares add rather than over-count:
        //   `ser`  the wire serialization (compact v5, or bincode)
        //   `hand` the datagram handoff — since threading Q1 the staging into
        //          the producer's batch for the path's I/O owner, not quinn's
        //          `send_datagram` (which the owner calls; see
        //          `net::cpuprof` module docs).
        use crate::net::cpuprof::{timed, Seam};
        if crate::transport::protocol::wire_compact_active() {
            let compact = timed(Seam::Ser, || {
                crate::transport::protocol::serialize_data_compact(&batch)
            });
            if let Some(buf) = compact {
                return timed(Seam::Hand, || self.stage(out, path_id, Lane::Data, buf.into()));
            }
        }
        let msg = WireMessage::Data(batch);
        let data = timed(Seam::Ser, || msg.serialize())?;

        timed(Seam::Hand, || self.stage(out, path_id, Lane::Data, data.into()))
    }

    /// [`Self::send_symbols`] for ONE symbol without building a
    /// `SymbolBatch` — the window sender's per-datagram path. Stages exactly
    /// what `send_symbols(out, path_id, SymbolBatch::new(vec![sym.clone()],
    /// send_timestamp_us, seqs, path_id).with_eta(eta_rel_us))` stages, byte
    /// for byte (the compact frame through the same writer; the bincode
    /// fallback through that very call), and fails the same way on a
    /// missing path. The compact frame is carved from the caller's `arena`
    /// (see `protocol::serialize_symbol_compact_in`).
    #[allow(clippy::too_many_arguments)]
    pub fn send_symbol(
        &self,
        out: &mut TxBatch,
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
            return self.send_symbols(out, path_id, batch);
        }
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
        timed(Seam::Hand, || self.stage(out, path_id, Lane::Data, buf))
    }

    /// Send a control message as a datagram (best-effort, low latency),
    /// staged in `out`'s control lane.
    pub fn send_control_datagram(&self, out: &mut TxBatch, path_id: PathId, msg: ControlMessage) -> anyhow::Result<()> {
        let wire = WireMessage::Control(msg);
        let data = wire.serialize()?;
        self.stage(out, path_id, Lane::Ctrl, data.into())
    }

    /// Send a control message over a path's reliable stream (the owner opens
    /// a uni stream, bounded by `io_owner::CONTROL_SEND_TIMEOUT`).
    pub async fn send_control(
        &self,
        path_id: PathId,
        msg: ControlMessage,
    ) -> anyhow::Result<()> {
        let io = self
            .io(path_id)
            .ok_or_else(|| anyhow::anyhow!("no connection on path {path_id}"))?;
        let wire = WireMessage::Control(msg);
        let data = wire.serialize()?;
        let (reply, rx) = tokio::sync::oneshot::channel();
        io.tx
            .send(IoCmd::Stream { data: data.into(), reply })
            .await
            .map_err(|_| anyhow::anyhow!("path {path_id}: the I/O owner is gone"))?;
        rx.await.map_err(|_| anyhow::anyhow!("path {path_id}: the I/O owner ended"))?
    }

    /// The max datagram size for a path (PMTU-based), from the owner's
    /// published snapshot.
    pub fn max_datagram_size(&self, path_id: PathId) -> Option<usize> {
        self.view(path_id)?.max_datagram_size()
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
            // The stronger off claim: the owner never ran the audit, so every
            // per-path audit counter is untouched.
            use std::sync::atomic::Ordering::Relaxed;
            let v = t.view(0).expect("path 0 has an owner");
            let a = &v.audit;
            assert_eq!(
                (a.handoff.load(Relaxed), a.full.load(Relaxed), a.err.load(Relaxed), a.space.load(Relaxed)),
                (0, 0, 0, 0),
                "the audit must not run when RWM_DIAG is off"
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
            t.spawn_path(7, any).await.expect("add_path's bind");
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

#[cfg(test)]
mod owner_rule_tests {
    /// Non-comment source lines of `src/**.rs` before each file's first
    /// `#[cfg(test)]`, as `(path relative to src/, line)`.
    fn code_lines() -> Vec<(String, String)> {
        fn walk(dir: &std::path::Path, root: &std::path::Path, out: &mut Vec<(String, String)>) {
            for e in std::fs::read_dir(dir).expect("read src dir").flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, root, out);
                } else if p.extension().is_some_and(|x| x == "rs") {
                    let src = std::fs::read_to_string(&p).expect("read source");
                    let lines: Vec<&str> = src.lines().collect();
                    // The non-test part ends at the first column-0 test
                    // attribute that opens a test MODULE (an item-level
                    // `#[cfg(test)]` — a test-only enum variant or helper
                    // fn — does not end it).
                    let end = (0..lines.len())
                        .find(|&i| {
                            (lines[i].starts_with("#[cfg(test)]") || lines[i].starts_with("#[cfg(all(test"))
                                && lines.get(i + 1).is_some_and(|n| {
                                    let n = n.trim_start_matches("pub(crate) ").trim_start_matches("pub ");
                                    n.starts_with("mod ")
                                })
                        })
                        .unwrap_or(lines.len());
                    let rel = p.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
                    for l in &lines[..end] {
                        let t = l.trim_start();
                        if !t.starts_with("//") {
                            out.push((rel.clone(), l.to_string()));
                        }
                    }
                }
            }
        }
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut out = Vec::new();
        walk(&root, &root, &mut out);
        assert!(out.len() > 10_000, "the scan read too little source to mean anything");
        out
    }

    /// Threading Q1, rule 5: no `quinn::Connection` is reachable outside the
    /// path's I/O owner. Source scan over every non-test, non-comment line of
    /// `src/`:
    ///
    /// * the type `quinn::Connection` and every connection-level quinn call
    ///   (`send_datagram`, `read_datagram`, the stream opens/accepts,
    ///   `datagram_send_buffer_space`) appear in `transport/io_owner.rs`
    ///   only;
    /// * inside it, the raw connection (`.raw.`) is touched only inside
    ///   `impl OwnedConn` — the single, identity-checked wrapper — and every
    ///   one of its quinn calls sits behind a `self.enter(..)` check;
    /// * the `connections` DashMap and its `conn()` accessor are gone from
    ///   the transport (P1's `connections_are_reached_through_one_guard_
    ///   dropping_accessor` asserted the map's existence; this test replaces
    ///   it).
    ///
    /// Red on main 69fd846: `quic.rs` holds `connections:
    /// DashMap<PathId, Arc<quinn::Connection>>` and calls `send_datagram`
    /// itself; `l0_netem.rs` calls `conn.send_datagram` from its own task.
    #[test]
    fn no_quinn_connection_is_reachable_outside_the_owner() {
        let lines = code_lines();
        let owner = "transport/io_owner.rs";
        let calls = [
            "quinn::Connection",
            ".send_datagram(",
            ".read_datagram(",
            ".open_uni(",
            ".accept_uni(",
            ".open_bi(",
            ".accept_bi(",
            ".datagram_send_buffer_space(",
        ];
        let outside: Vec<String> = lines
            .iter()
            .filter(|(f, l)| f != owner && calls.iter().any(|c| l.contains(c)))
            .map(|(f, l)| format!("{f}: {}", l.trim()))
            .collect();
        assert!(
            outside.is_empty(),
            "connection-level quinn use outside the owner:\n{}",
            outside.join("\n")
        );

        let quic: String = lines
            .iter()
            .filter(|(f, _)| f == "transport/quic.rs")
            .map(|(_, l)| l.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let squashed: String = quic.chars().filter(|c| !c.is_whitespace()).collect();
        assert!(!squashed.contains("connections:DashMap"), "the connections map must be gone");
        assert!(!squashed.contains("fnconn("), "the conn() accessor must be gone");

        // The raw connection only inside the wrapper's impl block.
        let own: Vec<&str> = lines
            .iter()
            .filter(|(f, _)| f == owner)
            .map(|(_, l)| l.as_str())
            .collect();
        let start = own
            .iter()
            .position(|l| l.starts_with("impl OwnedConn {"))
            .expect("the wrapper impl exists");
        let end = start
            + own[start..]
                .iter()
                .position(|l| *l == "}")
                .expect("the wrapper impl closes");
        let stray: Vec<&str> = own
            .iter()
            .enumerate()
            .filter(|(i, l)| (*i < start || *i > end) && l.contains(".raw."))
            .map(|(_, l)| *l)
            .collect();
        assert!(
            stray.is_empty(),
            "raw connection use outside `impl OwnedConn`:\n{}",
            stray.join("\n")
        );
        // Every method of the wrapper that reaches the raw connection opens
        // with the identity check.
        let body = &own[start..=end];
        let mut fns = 0;
        let mut i = 0;
        while i < body.len() {
            if body[i].trim_start().starts_with("fn ") || body[i].trim_start().starts_with("async fn ") {
                let j = (i + 1..body.len())
                    .find(|&j| body[j].trim_start().starts_with("fn ") || body[j].trim_start().starts_with("async fn "))
                    .unwrap_or(body.len());
                let f = &body[i..j];
                if f.iter().any(|l| l.contains("self.raw.")) {
                    fns += 1;
                    assert!(
                        f.iter().any(|l| l.contains("self.enter(")),
                        "wrapper method without the identity check: {}",
                        body[i].trim()
                    );
                }
                i = j;
            } else {
                i += 1;
            }
        }
        assert!(fns >= 10, "the wrapper must carry the quinn calls ({fns} methods found)");
    }
}

#[cfg(test)]
mod owner_runtime_tests {
    use super::*;

    /// The identity check is what the wrapper runs: inside the owner task of
    /// the path the check passes; outside any owner, or inside another
    /// path's owner, it fails.
    #[test]
    fn the_identity_rule_admits_only_the_owner() {
        use io_owner::identity_ok;
        assert!(identity_ok(3, Some(3)));
        assert!(!identity_ok(3, None), "outside any owner task");
        assert!(!identity_ok(3, Some(4)), "inside another path's owner");
    }
}
