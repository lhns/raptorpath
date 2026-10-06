//! Threading Q1 — the per-path I/O owner.
//!
//! **The owner rule (plan v2 rule 5).** One tokio task per path is the only
//! code that calls quinn for that path: it binds the path's `Endpoint`,
//! connects or accepts, runs the handshake, sends every datagram, reads every
//! datagram and uni stream, writes the reliable control streams, and answers
//! every MTU / RTT / stats question by *publishing* the answers into a
//! [`PathView`] the rest of the engine reads (rule 2: cross-direction reads
//! use published state). No `quinn::Connection` exists outside this module:
//! the connection is born inside the owner task and dies there.
//!
//! **Placement.** The owner is a task on the main runtime: the endpoint is
//! bound, the connection set up and quinn's drivers spawned there (quinn
//! 0.11.9 spawns its EndpointDriver and ConnectionDriver with a bare
//! `tokio::spawn`, and the socket registers with the ambient reactor). The
//! Q1 battery measured a dedicated-runtime placement (`RWM_IO_RT=own`)
//! against it; `own` failed at c1d (status §13) and was deleted.
//!
//! **Batched hops (rule 3).** Producers (the sender, the receiver, the control
//! fast path) stage serialized datagrams per path in a [`TxBatch`] they own and
//! hand each path's batch to its owner once per loop iteration — one
//! [`IoCmd::Send`] per Law-0 burst — over ONE bounded channel per path
//! ([`IO_CHANNEL_DEPTH`]); the server receiver's finished WindowAcks ride the
//! same channel in the control lane ([`Lane::Ctrl`]). The owner drains its
//! channel with `recv_many` (one coop-budget unit per call, never per
//! message) and forwards inbound datagrams to the receiver as ONE `Vec` per
//! owner poll. Wake coalescing comes from the channels themselves (rule 4): a
//! busy owner is not parked on its receiver, so a producer's send wakes
//! nothing.
//!
//! **One task per loop (rule 1).** The owner is one loop around one
//! `select!`; the uni-stream read and the control-stream write are each ONE
//! pending future slot in it (cold: every 2 s), not a sub-executor.
//!
//! **The P2a lesson** (status §12 addendum): never multiplex loops into one
//! task, never charge the coop budget per message.

use std::collections::VecDeque;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, OnceLock, Weak};
use std::task::Poll;
use std::time::{Duration, Instant};

use bytes::Bytes;
use quinn::{ClientConfig, Endpoint, ServerConfig};
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, error, info, warn};

use super::l0_netem::L0Netem;
use super::protocol::{Handshake, WireMessage, PROTOCOL_VERSION};
use super::rcvbuf::{self, RcvbufGrant};
use crate::scheduler::PathId;

// ───────────────────────────────────────────────────────────────────────────
// Constants (docs/fec-arq-model.md §11.2 provenance)

/// Depth of each path's owner channel, in batches.
///
/// Provenance: a resource bound stated outside any law. One batch is at most
/// one producer loop iteration's output (the sender's Law-0 burst, ≤
/// `RWM_EMIT_BURST` = 64 symbols ≈ 80 KB, plus that iteration's repairs; the
/// receiver's acks for one `recv_many` drain). A path has four producers
/// (sender, receiver, control fast path, report); 2 × 4 = 8 gives each of
/// them double buffering (one batch being consumed while the next is filled).
/// 8 × 80 KB ≈ 0.6 MB, ≤ 15 % of quinn's 4 MiB datagram send buffer
/// (`transport/quic.rs`), so queueing stays where the `RWM_DIAG` send-queue
/// audit can see it. A full channel back-pressures the producer
/// (`send().await`); it never drops.
pub const IO_CHANNEL_DEPTH: usize = 8;

/// Minimum age of the owner's full [`PathView`] snapshot (`stats()` +
/// `max_datagram_size()`) before an owner poll that did I/O refreshes it.
/// It is NOT a timer: the refresh rides on activity, so an idle owner never
/// wakes for it.
///
/// Provenance: tokio's timer resolution — every deadline is rounded up to
/// the next millisecond (tokio 1.50 `time/source.rs`), so no timer-clocked
/// reader can observe a snapshot fresher than 1 ms; every reader of the full
/// snapshot is clocked at ≥ 250 ms (`[DIAG]`, `[CTLD]`, `[WEDGE]`, the 2 s
/// report). The per-ack reader (`wire_rtt` under `RWM_COPA_WIRE`) is
/// refreshed every owner poll instead.
pub const VIEW_REFRESH: Duration = Duration::from_millis(1);

/// The most inbound datagrams one owner poll forwards as one batch.
///
/// Provenance: quinn-udp 0.5.14's `BATCH_SIZE = 32` (`unix.rs`: the most
/// datagrams one `recvmmsg` returns, i.e. the most the EndpointDriver
/// delivers per receive call). With it the inbound channel's batch depth is
/// `MSG_CHANNEL_DEPTH / INBOUND_BATCH_MAX`, so its worst case in datagrams
/// is ADR-0011's 4096, unchanged from the per-datagram channel. Binds are
/// counted (`rx_capped` on `[IOWN]`).
pub const INBOUND_BATCH_MAX: usize = 32;

/// Hard deadline on one reliable control-stream write (`open_uni` +
/// `write_all` + `finish`). Provenance: the report task's existing 500 ms
/// deadline on control sends (`net/tasks/report.rs`), now applied at the
/// operation inside the owner so a credit-starved stream cannot hold the
/// owner's single stream slot forever.
pub const CONTROL_SEND_TIMEOUT: Duration = Duration::from_millis(500);

/// Inbound channel item: one owner poll's datagrams, already decoded.
pub type InboundBatch = Vec<(PathId, WireMessage)>;

// ───────────────────────────────────────────────────────────────────────────
// Thread tokens (the routing probe)

/// A small, stable, nonzero id for the calling OS thread.
pub fn thread_token() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    thread_local! {
        static T: u64 = NEXT.fetch_add(1, Relaxed);
    }
    T.with(|t| *t)
}

// ───────────────────────────────────────────────────────────────────────────
// The published view

/// The datagram send-queue audit (`RWM_DIAG`), one per path.
///
/// **The gap this closes.** `quinn::Connection::send_datagram` calls
/// `Datagrams::send(data, drop = true)` (quinn-proto 0.11.14,
/// `connection/datagrams.rs`:38–48), which silently evicts the oldest
/// queued datagrams when the 4 MB send buffer overflows — about 3 300
/// 1 200 B symbols — logging a `trace!` nobody enables and returning `Ok`.
/// The engine's `src=`/`cod=` gauges count handoffs, not transmissions, so
/// an evicted symbol is indistinguishable from a delivered one.
///
/// **It cannot be counted exactly from quinn's public API.** What is exposed
/// is `Connection::datagram_send_buffer_space()` =
/// `datagram_send_buffer_size.saturating_sub(outgoing_total)`, read by the
/// owner immediately before each send:
///
/// * quinn's eviction loop runs iff `outgoing_total > buffer_size` on entry;
/// * `space == 0` iff `outgoing_total >= buffer_size` on entry.
///
/// So `full` is an upper bound on the number of evicting calls, tight to one
/// boundary case, and a lower bound on the datagrams evicted. The
/// corroborating cross-check is `tx_frames` (quinn's own
/// `stats().frame_tx.datagram`): DATAGRAM frames are never retransmitted, so
/// in a run that ends with a drained queue `handoff − tx_frames` is the total
/// lost to eviction.
///
/// **Off-value property:** with `RWM_DIAG` unset the audit does not run and
/// every counter reads 0 (`datagram_queue_stats` returns `None`).
///
/// **Scope:** only real `send_datagram` calls are audited; the `RWM_L0_NETEM`
/// shim has its own transit ledger.
#[derive(Default)]
pub struct DatagramQueueAudit {
    /// Sends quinn accepted (returned `Ok`).
    pub handoff: AtomicU64,
    /// Sends whose `datagram_send_buffer_space()` was 0 on entry.
    pub full: AtomicU64,
    /// Sends quinn rejected (`TooLarge` / `UnsupportedByPeer` / `Disabled` /
    /// `ConnectionLost`).
    pub err: AtomicU64,
    /// `datagram_send_buffer_space()` at the most recent send, in bytes.
    pub space: AtomicU64,
}

/// The owner's own counters. Each is written by the owner task only (no
/// cache line is shared with a producer's hot path); readers sum them at
/// report time.
#[derive(Default)]
pub struct OwnerCounters {
    /// Owner loop iterations (one `select!` resolution each).
    pub polls: AtomicU64,
    /// `recv_many` drains of the owner channel.
    pub drains: AtomicU64,
    /// Data-lane batches and their datagrams.
    pub tx_batches: AtomicU64,
    pub tx_dgrams: AtomicU64,
    /// Control-lane batches and their datagrams.
    pub ctrl_batches: AtomicU64,
    pub ctrl_dgrams: AtomicU64,
    /// Inbound batches forwarded to the receiver, and their datagrams.
    pub rx_batches: AtomicU64,
    pub rx_dgrams: AtomicU64,
    /// Inbound batches cut at `INBOUND_BATCH_MAX` (the cap's bind count).
    pub rx_capped: AtomicU64,
    /// Datagrams quinn rejected at `send_datagram`.
    pub send_err: AtomicU64,
    /// Datagrams a producer's stage refused because they exceeded the path's
    /// last published `max_datagram_size` (the synchronous `TooLarge`
    /// pre-check; written by the producer, rare). A datagram that passes the
    /// pre-check but meets a smaller MTU at quinn (the MTU dropped between
    /// publications) is quinn's `TooLarge` at the owner: counted in
    /// `send_err` (and the `RWM_DIAG` audit's `err`), never silent.
    pub too_large_staged: AtomicU64,
    /// Producer batches that found no owner (path removed between stage and
    /// flush), datagrams.
    pub orphaned: AtomicU64,
    /// Identity-checked quinn calls (the owner wrapper's check count).
    pub qcalls: AtomicU64,
    /// The lock-wait gauge (`RWM_RTOBS` only): owner sections that call
    /// quinn, their wall time and their thread CPU time (ns). Wall − CPU is
    /// the owner's wall time asleep inside quinn: the connection-mutex wait,
    /// plus any involuntary preemption inside a section.
    pub q_secs: AtomicU64,
    pub q_wall_ns: AtomicU64,
    pub q_cpu_ns: AtomicU64,
    /// Set when a section's thread CPU time could not be read (non-Linux):
    /// the asleep figure is then unavailable, printed `-`.
    pub q_cpu_unavailable: AtomicBool,
}

/// What the owner publishes about its path. Read by the sender, the
/// receiver, the report task and the instruments; written by the owner only
/// (and by the routing probe on the driver's thread).
pub struct PathView {
    pub path: PathId,
    /// Process-unique id (the `[IOWN]` window join key).
    pub serial: u64,
    /// `client` / `server`.
    pub side: &'static str,
    /// The runtime the owner runs on (`main`: the owner is a task on the
    /// main runtime).
    pub rt: String,
    /// quinn's RTT (µs), refreshed every owner poll under `RWM_COPA_WIRE` and
    /// with every full snapshot otherwise.
    rtt_us: AtomicU64,
    /// `max_datagram_size()` + 1 (0 = not yet known / unsupported).
    max_dgram_p1: AtomicU64,
    /// A full snapshot has been published.
    snap: AtomicBool,
    frame_rx_dgram: AtomicU64,
    frame_tx_dgram: AtomicU64,
    cwnd: AtomicU64,
    cong_events: AtomicU64,
    lost_packets: AtomicU64,
    sent_packets: AtomicU64,
    /// The `RWM_DIAG` send-queue audit.
    pub audit: DatagramQueueAudit,
    /// `RWM_DIAG` receive audit: datagrams the owner read from quinn.
    pub app_read: AtomicU64,
    /// Whether the readers were started (the receive audit's off value).
    pub readers_started: AtomicBool,
    pub ctr: OwnerCounters,
    /// The routing probe: the thread the owner last ran on, and the quinn
    /// ConnectionDriver's congestion-controller `on_sent` calls observed on
    /// that thread (`drv_on`) and elsewhere (`drv_off`). Armed with
    /// `RWM_RTOBS` (or explicitly by a test); 0/0 otherwise.
    pub owner_thread: AtomicU64,
    pub drv_on: AtomicU64,
    pub drv_off: AtomicU64,
    /// Emptied batch `Vec`s, returned by the owner for the producers' reuse.
    pool: parking_lot::Mutex<Vec<Vec<Bytes>>>,
}

impl PathView {
    pub fn new(path: PathId, side: &'static str, rt: String) -> Arc<Self> {
        static SERIAL: AtomicU64 = AtomicU64::new(1);
        let v = Arc::new(Self {
            path,
            serial: SERIAL.fetch_add(1, Relaxed),
            side,
            rt,
            rtt_us: AtomicU64::new(0),
            max_dgram_p1: AtomicU64::new(0),
            snap: AtomicBool::new(false),
            frame_rx_dgram: AtomicU64::new(0),
            frame_tx_dgram: AtomicU64::new(0),
            cwnd: AtomicU64::new(0),
            cong_events: AtomicU64::new(0),
            lost_packets: AtomicU64::new(0),
            sent_packets: AtomicU64::new(0),
            audit: DatagramQueueAudit::default(),
            app_read: AtomicU64::new(0),
            readers_started: AtomicBool::new(false),
            ctr: OwnerCounters::default(),
            owner_thread: AtomicU64::new(0),
            drv_on: AtomicU64::new(0),
            drv_off: AtomicU64::new(0),
            pool: parking_lot::Mutex::new(Vec::new()),
        });
        registry().lock().push(Arc::downgrade(&v));
        v
    }

    /// quinn's RTT for this path, `None` before the first publish.
    pub fn rtt(&self) -> Option<Duration> {
        self.snap
            .load(Relaxed)
            .then(|| Duration::from_micros(self.rtt_us.load(Relaxed)))
    }

    /// `max_datagram_size()` at the last full snapshot.
    pub fn max_datagram_size(&self) -> Option<usize> {
        match self.max_dgram_p1.load(Relaxed) {
            0 => None,
            v => Some((v - 1) as usize),
        }
    }

    /// `(frame_rx.datagram, frame_tx.datagram)` at the last full snapshot.
    pub fn frame_stats(&self) -> Option<(u64, u64)> {
        self.snap
            .load(Relaxed)
            .then(|| (self.frame_rx_dgram.load(Relaxed), self.frame_tx_dgram.load(Relaxed)))
    }

    /// `(cwnd, congestion_events, lost_packets, sent_packets)` at the last
    /// full snapshot.
    pub fn path_stats(&self) -> Option<(u64, u64, u64, u64)> {
        self.snap.load(Relaxed).then(|| {
            (
                self.cwnd.load(Relaxed),
                self.cong_events.load(Relaxed),
                self.lost_packets.load(Relaxed),
                self.sent_packets.load(Relaxed),
            )
        })
    }

    /// A batch `Vec` for a producer: a recycled one when available.
    pub fn take_vec(&self) -> Vec<Bytes> {
        self.pool.lock().pop().unwrap_or_default()
    }

    fn recycle(&self, mut v: Vec<Bytes>) {
        v.clear();
        if v.capacity() > 0 {
            self.pool.lock().push(v);
        }
    }
}

fn registry() -> &'static parking_lot::Mutex<Vec<Weak<PathView>>> {
    static R: OnceLock<parking_lot::Mutex<Vec<Weak<PathView>>>> = OnceLock::new();
    R.get_or_init(|| parking_lot::Mutex::new(Vec::new()))
}

/// Every live path view in the process (the `[IOWN]` readout).
pub fn live_views() -> Vec<Arc<PathView>> {
    let mut r = registry().lock();
    r.retain(|w| w.strong_count() > 0);
    r.iter().filter_map(|w| w.upgrade()).collect()
}

// ───────────────────────────────────────────────────────────────────────────
// The routing probe (a delegating congestion controller)

/// Wraps any controller factory: every controller it builds forwards each
/// call to the real controller and, on `on_sent` (called by quinn-proto's
/// `poll_transmit`, i.e. inside the ConnectionDriver's poll), records whether
/// it ran on the owner's thread. Behaviour is the inner controller's,
/// unchanged.
pub(crate) struct RouteProbeFactory {
    pub inner: Arc<dyn quinn::congestion::ControllerFactory + Send + Sync + 'static>,
    pub view: Arc<PathView>,
}

impl quinn::congestion::ControllerFactory for RouteProbeFactory {
    fn build(
        self: Arc<Self>,
        now: Instant,
        current_mtu: u16,
    ) -> Box<dyn quinn::congestion::Controller> {
        Box::new(RouteProbe {
            inner: self.inner.clone().build(now, current_mtu),
            view: self.view.clone(),
        })
    }
}

struct RouteProbe {
    inner: Box<dyn quinn::congestion::Controller>,
    view: Arc<PathView>,
}

impl quinn::congestion::Controller for RouteProbe {
    fn on_sent(&mut self, now: Instant, bytes: u64, last_packet_number: u64) {
        if self.view.owner_thread.load(Relaxed) == thread_token() {
            self.view.drv_on.fetch_add(1, Relaxed);
        } else {
            self.view.drv_off.fetch_add(1, Relaxed);
        }
        self.inner.on_sent(now, bytes, last_packet_number)
    }

    fn on_ack(
        &mut self,
        now: Instant,
        sent: Instant,
        bytes: u64,
        app_limited: bool,
        rtt: &quinn_proto::RttEstimator,
    ) {
        self.inner.on_ack(now, sent, bytes, app_limited, rtt)
    }

    fn on_end_acks(
        &mut self,
        now: Instant,
        in_flight: u64,
        app_limited: bool,
        largest_packet_num_acked: Option<u64>,
    ) {
        self.inner
            .on_end_acks(now, in_flight, app_limited, largest_packet_num_acked)
    }

    fn on_congestion_event(
        &mut self,
        now: Instant,
        sent: Instant,
        is_persistent_congestion: bool,
        lost_bytes: u64,
    ) {
        self.inner
            .on_congestion_event(now, sent, is_persistent_congestion, lost_bytes)
    }

    fn on_mtu_update(&mut self, new_mtu: u16) {
        self.inner.on_mtu_update(new_mtu)
    }

    fn window(&self) -> u64 {
        self.inner.window()
    }

    fn metrics(&self) -> quinn_proto::congestion::ControllerMetrics {
        self.inner.metrics()
    }

    fn clone_box(&self) -> Box<dyn quinn::congestion::Controller> {
        Box::new(RouteProbe { inner: self.inner.clone_box(), view: self.view.clone() })
    }

    fn initial_window(&self) -> u64 {
        self.inner.initial_window()
    }

    fn into_any(self: Box<Self>) -> Box<dyn std::any::Any> {
        self.inner.into_any()
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Commands

/// The two lanes of the one owner channel. The lane tags the batch for the
/// counters; the owner serves both in arrival order (a control datagram never
/// overtakes the data staged before it — the sender's `Shutdown` follows its
/// last burst).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lane {
    Data,
    Ctrl,
}

/// One message to a path's owner.
pub(crate) enum IoCmd {
    /// Datagrams to send, in order.
    Send { lane: Lane, dgrams: Vec<Bytes> },
    /// One reliable control message (a framed `WireMessage`) on a fresh uni
    /// stream; the reply carries the outcome.
    Stream { data: Bytes, reply: oneshot::Sender<anyhow::Result<()>> },
    /// Start forwarding inbound datagrams (`msg_tx`) and uni-stream control
    /// messages (`ctrl_tx`).
    StartReaders {
        msg_tx: mpsc::Sender<InboundBatch>,
        ctrl_tx: mpsc::Sender<(PathId, WireMessage)>,
    },
    /// Connect to `peer` (client) or accept one connection (server), with the
    /// ADR-0010 handshake.
    Connect { peer: SocketAddr, reply: oneshot::Sender<anyhow::Result<()>> },
    Accept { reply: oneshot::Sender<anyhow::Result<()>> },
    /// Close the connection and end the owner.
    Close,
}

/// A path's handle as the rest of the engine holds it: the owner's channel
/// and its published view. No quinn object.
#[derive(Clone)]
pub(crate) struct PathIo {
    pub tx: mpsc::Sender<IoCmd>,
    pub view: Arc<PathView>,
}

// ───────────────────────────────────────────────────────────────────────────
// Owner identity: the single quinn wrapper

tokio::task_local! {
    /// The path whose owner is running the current task (set by
    /// [`run_owner`] around the whole owner future).
    static OWNER: PathId;
}

static ID_VIOLATIONS: AtomicU64 = AtomicU64::new(0);
static ID_FIRST: parking_lot::Mutex<Option<String>> = parking_lot::Mutex::new(None);

/// `(checks over every live view, violations)` of the owner-identity check.
pub fn identity_counts() -> (u64, u64) {
    let checks = live_views().iter().map(|v| v.ctr.qcalls.load(Relaxed)).sum();
    (checks, ID_VIOLATIONS.load(Relaxed))
}

/// The first recorded identity violation, if any.
pub fn first_identity_violation() -> Option<String> {
    ID_FIRST.lock().clone()
}

/// The identity rule the wrapper enforces: the caller runs inside the owner
/// task of `path` (`owner` = the task-local path, `None` outside any owner).
pub(crate) fn identity_ok(path: PathId, owner: Option<PathId>) -> bool {
    owner == Some(path)
}

/// The connection, reachable only through this wrapper. Every method checks
/// the caller's identity first: it must run inside the owner task of THIS
/// path (task-local); and (the P1 lock-order witness) with no scheduler
/// guard alive on the thread. A failed check panics.
pub(crate) struct OwnedConn {
    raw: quinn::Connection,
    path: PathId,
    view: Arc<PathView>,
}

impl OwnedConn {
    fn new(raw: quinn::Connection, path: PathId, view: Arc<PathView>) -> Self {
        Self { raw, path, view }
    }

    #[inline]
    #[track_caller]
    fn enter(&self, seam: &'static str) {
        crate::scheduler::sched_lock::assert_not_held(seam);
        self.view.ctr.qcalls.fetch_add(1, Relaxed);
        let owner = OWNER.try_with(|p| *p).ok();
        if !identity_ok(self.path, owner) {
            ID_VIOLATIONS.fetch_add(1, Relaxed);
            let at = std::panic::Location::caller();
            let msg = format!(
                "io owner: quinn seam `{seam}` ({at}) for path {} called outside its owner \
                 (running in owner {owner:?})",
                self.path
            );
            ID_FIRST.lock().get_or_insert_with(|| msg.clone());
            panic!("{msg}");
        }
    }

    fn send_datagram(&self, b: Bytes) -> Result<(), quinn::SendDatagramError> {
        self.enter("send_datagram");
        self.raw.send_datagram(b)
    }

    fn datagram_send_buffer_space(&self) -> usize {
        self.enter("datagram_send_buffer_space");
        self.raw.datagram_send_buffer_space()
    }

    fn read_datagram(&self) -> impl Future<Output = Result<Bytes, quinn::ConnectionError>> + Send + '_ {
        self.enter("read_datagram");
        self.raw.read_datagram()
    }

    fn rtt(&self) -> Duration {
        self.enter("rtt");
        self.raw.rtt()
    }

    fn stats(&self) -> quinn::ConnectionStats {
        self.enter("stats");
        self.raw.stats()
    }

    fn max_datagram_size(&self) -> Option<usize> {
        self.enter("max_datagram_size");
        self.raw.max_datagram_size()
    }

    fn close(&self, reason: &[u8]) {
        self.enter("close");
        self.raw.close(0u32.into(), reason);
    }

    fn remote_address(&self) -> SocketAddr {
        self.enter("remote_address");
        self.raw.remote_address()
    }

    /// Accept one uni stream and read one framed control message from it.
    /// `Err(())`: the connection's stream accept ended (the reader stops);
    /// `Ok(None)`: this stream was malformed or short (skip it).
    async fn read_uni(&self) -> Result<Option<WireMessage>, ()> {
        self.enter("accept_uni");
        let mut recv = match self.raw.accept_uni().await {
            Ok(r) => r,
            Err(e) => {
                error!(path_id = self.path, ?e, "uni stream accept error");
                return Err(());
            }
        };
        debug!(path_id = self.path, "uni stream accepted");
        let mut len_buf = [0u8; 4];
        if let Err(e) = recv.read_exact(&mut len_buf).await {
            debug!(path_id = self.path, ?e, "uni stream length read failed");
            return Ok(None);
        }
        let len = u32::from_be_bytes(len_buf) as usize;
        if len > 1_000_000 {
            return Ok(None);
        }
        let mut data = vec![0u8; len];
        if recv.read_exact(&mut data).await.is_err() {
            return Ok(None);
        }
        match WireMessage::deserialize(&data) {
            Ok(msg) => Ok(Some(msg)),
            Err(e) => {
                warn!(path_id = self.path, ?e, "failed to deserialize uni stream message");
                Ok(None)
            }
        }
    }

    /// One reliable control message on a fresh uni stream (length-prefixed).
    async fn write_uni(&self, data: Bytes) -> anyhow::Result<()> {
        self.enter("open_uni");
        let mut send = self.raw.open_uni().await?;
        send.write_all(&(data.len() as u32).to_be_bytes()).await?;
        send.write_all(&data).await?;
        send.finish()?;
        Ok(())
    }

    /// ADR-0010 handshake, client side. Returns the peer's handshake.
    async fn perform_handshake(&self, local: &Handshake) -> anyhow::Result<Handshake> {
        self.enter("open_bi");
        let (mut send, mut recv) = self.raw.open_bi().await?;
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

    /// ADR-0010 handshake, server side. Returns the peer's handshake.
    async fn accept_handshake(&self, local: &Handshake) -> anyhow::Result<Handshake> {
        self.enter("accept_bi");
        let (mut send, mut recv) = self.raw.accept_bi().await?;
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
}

fn local_handshake(path_id: PathId) -> Handshake {
    Handshake { version: PROTOCOL_VERSION, max_block_size: 64 * 1024, symbol_size: 1200, path_id }
}

// ───────────────────────────────────────────────────────────────────────────
// The lock-wait gauge

#[cfg(target_os = "linux")]
fn thread_cpu_ns() -> Option<u64> {
    let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    // SAFETY: a valid out-pointer; CLOCK_THREAD_CPUTIME_ID has no other
    // precondition.
    let r = unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut ts) };
    (r == 0).then(|| ts.tv_sec as u64 * 1_000_000_000 + ts.tv_nsec as u64)
}

#[cfg(not(target_os = "linux"))]
fn thread_cpu_ns() -> Option<u64> {
    None
}

/// One measured owner section (a stretch of the owner's poll that calls
/// quinn). `None` when the gauge is off.
struct Section {
    wall: Instant,
    cpu: Option<u64>,
}

fn section_begin(on: bool) -> Option<Section> {
    on.then(|| Section { wall: Instant::now(), cpu: thread_cpu_ns() })
}

fn section_end(ctr: &OwnerCounters, s: Option<Section>) {
    let Some(s) = s else { return };
    let wall = s.wall.elapsed().as_nanos() as u64;
    ctr.q_secs.fetch_add(1, Relaxed);
    ctr.q_wall_ns.fetch_add(wall, Relaxed);
    match (s.cpu, thread_cpu_ns()) {
        (Some(a), Some(b)) => {
            ctr.q_cpu_ns.fetch_add(b.saturating_sub(a).min(wall), Relaxed);
        }
        _ => ctr.q_cpu_unavailable.store(true, Relaxed),
    }
}

// ───────────────────────────────────────────────────────────────────────────
// The owner task

/// Bind one path's endpoint socket with the receive-buffer request
/// (transport/rcvbuf.rs), echo the grant, and hand the socket to quinn — on
/// the CURRENT runtime (the owner's), so quinn's EndpointDriver is spawned
/// there and the socket registers with its reactor.
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
    let runtime = quinn::default_runtime().ok_or_else(|| anyhow::anyhow!("no async runtime found"))?;
    let ep = Endpoint::new(quinn::EndpointConfig::default(), server_config, sock, runtime)?;
    Ok((ep, probe, grant))
}

/// Everything an owner is born with.
pub(crate) struct OwnerArgs {
    pub path: PathId,
    pub bind: SocketAddr,
    pub server_config: Option<ServerConfig>,
    pub client_config: Option<ClientConfig>,
    pub view: Arc<PathView>,
    pub cmd_rx: mpsc::Receiver<IoCmd>,
    pub bound: Option<oneshot::Sender<anyhow::Result<(std::net::UdpSocket, RcvbufGrant)>>>,
    pub(super) shim: Option<Arc<L0Netem>>,
    pub is_server: bool,
    /// `RWM_DIAG`: the per-send queue audit.
    pub dg_audit: bool,
    /// `RWM_RTOBS`: the lock-wait gauge's clock reads.
    pub gauge: bool,
}

/// The owner task. Spawned on the main runtime; the identity task-local is
/// set around the whole future.
pub(crate) async fn run_owner(a: OwnerArgs) {
    let path = a.path;
    OWNER.scope(path, owner_body(a)).await
}

async fn owner_body(mut a: OwnerArgs) {
    a.view.owner_thread.store(thread_token(), Relaxed);
    // 1. Bind, here, on the owner's runtime.
    let ep = match bind_endpoint(a.path, a.bind, a.server_config.take()) {
        Ok((mut ep, probe, grant)) => {
            if let Some(cc) = a.client_config.take() {
                ep.set_default_client_config(cc);
            }
            if let Some(b) = a.bound.take() {
                let _ = b.send(Ok((probe, grant)));
            }
            ep
        }
        Err(e) => {
            if let Some(b) = a.bound.take() {
                let _ = b.send(Err(e));
            }
            return;
        }
    };
    // 2. Set up the connection, here: quinn spawns its ConnectionDriver on
    //    this runtime.
    let mut early_readers: Option<Readers> = None;
    let conn = loop {
        let Some(cmd) = a.cmd_rx.recv().await else { return };
        let (res, reply) = match cmd {
            IoCmd::Connect { peer, reply } => (connect(&ep, &a, peer).await, reply),
            IoCmd::Accept { reply } => (accept(&ep, &a).await, reply),
            IoCmd::Close => return,
            IoCmd::Stream { reply, .. } => {
                let _ = reply.send(Err(anyhow::anyhow!("path {} is not connected", a.path)));
                continue;
            }
            IoCmd::Send { dgrams, .. } => {
                a.view.ctr.orphaned.fetch_add(dgrams.len() as u64, Relaxed);
                continue;
            }
            IoCmd::StartReaders { msg_tx, ctrl_tx } => {
                early_readers = Some(Readers { msg_tx, ctrl_tx });
                continue;
            }
        };
        match res {
            Ok(c) => {
                let _ = reply.send(Ok(()));
                break c;
            }
            Err(e) => {
                let _ = reply.send(Err(e));
            }
        }
    };
    serve(a, &conn, early_readers).await;
    drop(conn);
    drop(ep);
}

async fn connect(ep: &Endpoint, a: &OwnerArgs, peer: SocketAddr) -> anyhow::Result<OwnedConn> {
    let raw = ep.connect(peer, "raptorpath")?.await?;
    let conn = OwnedConn::new(raw, a.path, a.view.clone());
    let _peer_hs = conn.perform_handshake(&local_handshake(a.path)).await?;
    info!(path_id = a.path, %peer, "connected and handshake complete");
    Ok(conn)
}

async fn accept(ep: &Endpoint, a: &OwnerArgs) -> anyhow::Result<OwnedConn> {
    let incoming = ep.accept().await.ok_or_else(|| anyhow::anyhow!("endpoint closed"))?;
    let raw = incoming.await?;
    let conn = OwnedConn::new(raw, a.path, a.view.clone());
    let _peer_hs = conn.accept_handshake(&local_handshake(a.path)).await?;
    info!(path_id = a.path, remote = %conn.remote_address(), "accepted with handshake");
    Ok(conn)
}

struct Readers {
    msg_tx: mpsc::Sender<InboundBatch>,
    ctrl_tx: mpsc::Sender<(PathId, WireMessage)>,
}

/// Poll `f` once with the task's own context: `Some(output)` if it is ready
/// now, `None` otherwise (its waker is then registered, harmlessly).
async fn poll_once<F: Future>(f: F) -> Option<F::Output> {
    let mut f = std::pin::pin!(f);
    std::future::poll_fn(|cx| {
        Poll::Ready(match f.as_mut().poll(cx) {
            Poll::Ready(v) => Some(v),
            Poll::Pending => None,
        })
    })
    .await
}

type BoxFut<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Rate-limited warning: the 1st, 2nd, 4th, 8th, … occurrence (D18: no
/// per-datagram logging on a hot task).
fn warn_pow2(n: u64) -> bool {
    n.is_power_of_two()
}

/// The owner's steady state: one loop around one `select!`.
async fn serve(mut a: OwnerArgs, conn: &OwnedConn, early_readers: Option<Readers>) {
    let path = a.path;
    let view = a.view.clone();
    let ctr = &view.ctr;
    let copa_wire = crate::scheduler::copa_wire_active();
    let mut cmds: Vec<IoCmd> = Vec::with_capacity(IO_CHANNEL_DEPTH);
    let mut readers: Option<Readers> = None;
    if let Some(r) = early_readers {
        view.readers_started.store(true, Relaxed);
        readers = Some(r);
    }
    let mut reader_dead = false;
    let mut inbound: InboundBatch = Vec::new();
    let mut ctrl_pending: Option<(PathId, WireMessage)> = None;
    let mut uni: Option<BoxFut<'_, Result<Option<WireMessage>, ()>>> = None;
    let mut uni_dead = false;
    let mut streams: VecDeque<(Bytes, oneshot::Sender<anyhow::Result<()>>)> = VecDeque::new();
    let mut stream_fut: Option<BoxFut<'_, anyhow::Result<()>>> = None;
    let mut stream_reply: Option<oneshot::Sender<anyhow::Result<()>>> = None;
    // The L0 shim's per-path FIFO release queue (release µs on the shim's
    // clock, datagram) — it lives here so its delayed sends are the owner's.
    let mut shim_q: VecDeque<(u64, Bytes)> = VecDeque::new();
    let far = tokio::time::Instant::now() + Duration::from_secs(86_400);
    let shim_sleep = tokio::time::sleep_until(far);
    tokio::pin!(shim_sleep);
    let mut shim_armed: Option<u64> = None;
    // The view refresh is driven by activity only (no timer: an idle owner
    // never wakes for it), rate-limited by snapshot age.
    let mut last_full = Instant::now();
    let mut dirty = false;
    publish(conn, &view, true, copa_wire, a.gauge);

    loop {
        view.owner_thread.store(thread_token(), Relaxed);
        if readers.is_some() && !uni_dead && uni.is_none() && ctrl_pending.is_none() {
            uni = Some(Box::pin(conn.read_uni()));
        }
        if stream_fut.is_none() {
            if let Some((data, reply)) = streams.pop_front() {
                stream_fut = Some(Box::pin(async move {
                    tokio::time::timeout(CONTROL_SEND_TIMEOUT, conn.write_uni(data))
                        .await
                        .map_err(|_| anyhow::anyhow!("control stream send timed out (stream credit?)"))?
                }));
                stream_reply = Some(reply);
            }
        }
        if let (Some(shim), Some(&(rel, _))) = (a.shim.as_ref(), shim_q.front()) {
            if shim_armed != Some(rel) {
                shim_sleep
                    .as_mut()
                    .reset(tokio::time::Instant::from_std(shim.deadline(rel)));
                shim_armed = Some(rel);
            }
        }
        let reading = readers.is_some() && !reader_dead && inbound.is_empty();
        let mut close = false;
        tokio::select! {
            n = a.cmd_rx.recv_many(&mut cmds, IO_CHANNEL_DEPTH) => {
                if n == 0 {
                    break; // every producer handle is gone
                }
                ctr.drains.fetch_add(1, Relaxed);
                let sec = section_begin(a.gauge);
                for cmd in cmds.drain(..) {
                    match cmd {
                        IoCmd::Send { lane, mut dgrams } => {
                            let n = dgrams.len() as u64;
                            match lane {
                                Lane::Data => {
                                    ctr.tx_batches.fetch_add(1, Relaxed);
                                    ctr.tx_dgrams.fetch_add(n, Relaxed);
                                }
                                Lane::Ctrl => {
                                    ctr.ctrl_batches.fetch_add(1, Relaxed);
                                    ctr.ctrl_dgrams.fetch_add(n, Relaxed);
                                }
                            }
                            for b in dgrams.drain(..) {
                                send_one(conn, &a, &mut shim_q, b);
                            }
                            view.recycle(dgrams);
                            dirty = true;
                        }
                        IoCmd::Stream { data, reply } => streams.push_back((data, reply)),
                        IoCmd::StartReaders { msg_tx, ctrl_tx } => {
                            view.readers_started.store(true, Relaxed);
                            readers = Some(Readers { msg_tx, ctrl_tx });
                            reader_dead = false;
                        }
                        IoCmd::Connect { reply, .. } | IoCmd::Accept { reply } => {
                            let _ = reply.send(Err(anyhow::anyhow!("path {path} is already connected")));
                        }
                        IoCmd::Close => close = true,
                    }
                }
                section_end(ctr, sec);
            }
            r = conn.read_datagram(), if reading => {
                match r {
                    Ok(d) => {
                        let sec = section_begin(a.gauge);
                        push_inbound(&mut inbound, &view, a.dg_audit, path, d);
                        // What quinn already buffered, in this poll, up to
                        // INBOUND_BATCH_MAX (the cap's binds are counted).
                        loop {
                            if inbound.len() >= INBOUND_BATCH_MAX {
                                ctr.rx_capped.fetch_add(1, Relaxed);
                                break;
                            }
                            match poll_once(conn.read_datagram()).await {
                                Some(Ok(d)) => push_inbound(&mut inbound, &view, a.dg_audit, path, d),
                                Some(Err(e)) => {
                                    error!(path_id = path, ?e, "datagram receive error");
                                    reader_dead = true;
                                    break;
                                }
                                None => break,
                            }
                        }
                        section_end(ctr, sec);
                        dirty = true;
                        if !inbound.is_empty() {
                            let n = inbound.len() as u64;
                            let tx = &readers.as_ref().expect("reading implies readers").msg_tx;
                            match tx.try_send(std::mem::take(&mut inbound)) {
                                Ok(()) => {
                                    ctr.rx_batches.fetch_add(1, Relaxed);
                                    ctr.rx_dgrams.fetch_add(n, Relaxed);
                                }
                                // Full: keep the batch; the reserve arm
                                // forwards it (reads pause meanwhile, so
                                // quinn's own bounded buffer holds the rest).
                                Err(mpsc::error::TrySendError::Full(b)) => inbound = b,
                                Err(mpsc::error::TrySendError::Closed(_)) => {
                                    readers = None;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!(path_id = path, ?e, "datagram receive error");
                        reader_dead = true;
                    }
                }
            }
            p = reserve_owned(readers.as_ref().map(|r| r.msg_tx.clone())), if !inbound.is_empty() => {
                match p {
                    Some(permit) => {
                        ctr.rx_batches.fetch_add(1, Relaxed);
                        ctr.rx_dgrams.fetch_add(inbound.len() as u64, Relaxed);
                        permit.send(std::mem::take(&mut inbound));
                    }
                    None => {
                        inbound.clear();
                        readers = None;
                    }
                }
            }
            r = async { uni.as_mut().expect("guarded").await }, if uni.is_some() => {
                uni = None;
                match r {
                    Ok(Some(m)) => ctrl_pending = Some((path, m)),
                    Ok(None) => {}
                    Err(()) => uni_dead = true,
                }
            }
            p = reserve_owned(readers.as_ref().map(|r| r.ctrl_tx.clone())), if ctrl_pending.is_some() => {
                let m = ctrl_pending.take().expect("guarded");
                if let Some(permit) = p {
                    permit.send(m);
                }
            }
            r = async { stream_fut.as_mut().expect("guarded").await }, if stream_fut.is_some() => {
                stream_fut = None;
                if let Some(reply) = stream_reply.take() {
                    let _ = reply.send(r);
                }
            }
            _ = &mut shim_sleep, if !shim_q.is_empty() => {
                if let Some(shim) = a.shim.as_ref() {
                    let now = shim.now_us();
                    while shim_q.front().is_some_and(|(rel, _)| *rel <= now) {
                        let (_, b) = shim_q.pop_front().expect("front");
                        let ok = conn.send_datagram(b).is_ok();
                        shim.released(path, ok);
                    }
                }
                shim_armed = None;
                dirty = true;
            }
        }
        ctr.polls.fetch_add(1, Relaxed);
        // Publish, on activity only: the per-ack RTT every poll (only its
        // consumer's gate), the full snapshot when this poll did I/O and the
        // last one is ≥ VIEW_REFRESH old. No timer: an idle owner never wakes
        // to refresh. The snapshot's age is therefore bounded by
        // max(VIEW_REFRESH, the gap since the path's last I/O) — and while a
        // path is idle quinn's datagram counters do not move with app
        // traffic (a transfer's tail still wakes the owner: the peer's acks
        // and the receiver's reads are I/O), so a stale idle snapshot reads
        // what a fresh one would, up to quinn-internal packets.
        let now = Instant::now();
        let full = dirty && now.duration_since(last_full) >= VIEW_REFRESH;
        if full || copa_wire {
            let sec = section_begin(a.gauge && full);
            publish(conn, &view, full, copa_wire, a.gauge);
            section_end(ctr, sec);
        }
        if full {
            last_full = now;
            dirty = false;
        }
        if close {
            break;
        }
    }
    // Answer any stream request still queued, then close.
    for (_, reply) in streams.drain(..) {
        let _ = reply.send(Err(anyhow::anyhow!("path {path} closed")));
    }
    drop(stream_fut);
    drop(uni);
    publish(conn, &view, true, copa_wire, a.gauge);
    conn.close(b"path removed");
}

/// `reserve_owned` on an optional sender: `None` when absent or closed.
async fn reserve_owned<T>(tx: Option<mpsc::Sender<T>>) -> Option<mpsc::OwnedPermit<T>> {
    match tx {
        Some(tx) => tx.reserve_owned().await.ok(),
        None => std::future::pending().await,
    }
}

fn push_inbound(inbound: &mut InboundBatch, view: &PathView, audit: bool, path: PathId, d: Bytes) {
    if audit {
        view.app_read.fetch_add(1, Relaxed);
    }
    match WireMessage::deserialize(&d) {
        Ok(msg) => inbound.push((path, msg)),
        Err(e) => warn!(path_id = path, ?e, "failed to deserialize datagram"),
    }
}

/// One datagram: through the L0 shim's queue when it is active, else the
/// audit (under `RWM_DIAG`) and `send_datagram`.
fn send_one(conn: &OwnedConn, a: &OwnerArgs, shim_q: &mut VecDeque<(u64, Bytes)>, b: Bytes) {
    if let Some(shim) = a.shim.as_ref() {
        if let Some(rel) = shim.shape(a.path, a.is_server, b.len()) {
            shim_q.push_back((rel, b));
        }
        return;
    }
    let audit = &a.view.audit;
    if a.dg_audit {
        // Read before the send: quinn's eviction decision is taken against
        // the state on entry.
        let space = conn.datagram_send_buffer_space() as u64;
        audit.space.store(space, Relaxed);
        if space == 0 {
            audit.full.fetch_add(1, Relaxed);
        }
    }
    match conn.send_datagram(b) {
        Ok(()) => {
            if a.dg_audit {
                audit.handoff.fetch_add(1, Relaxed);
            }
        }
        Err(e) => {
            if a.dg_audit {
                audit.err.fetch_add(1, Relaxed);
            }
            let n = a.view.ctr.send_err.fetch_add(1, Relaxed) + 1;
            if warn_pow2(n) {
                warn!(path_id = a.path, ?e, errors = n, "datagram send failed at the owner");
            }
        }
    }
}

/// Publish the view: RTT (every call under `RWM_COPA_WIRE`), and the full
/// snapshot when `full`.
fn publish(conn: &OwnedConn, view: &PathView, full: bool, copa_wire: bool, _gauge: bool) {
    if full {
        let s = conn.stats();
        view.rtt_us.store(s.path.rtt.as_micros() as u64, Relaxed);
        view.frame_rx_dgram.store(s.frame_rx.datagram, Relaxed);
        view.frame_tx_dgram.store(s.frame_tx.datagram, Relaxed);
        view.cwnd.store(s.path.cwnd, Relaxed);
        view.cong_events.store(s.path.congestion_events, Relaxed);
        view.lost_packets.store(s.path.lost_packets, Relaxed);
        view.sent_packets.store(s.path.sent_packets, Relaxed);
        let m = conn.max_datagram_size().map_or(0, |m| m as u64 + 1);
        view.max_dgram_p1.store(m, Relaxed);
        view.snap.store(true, Relaxed);
    } else if copa_wire {
        view.rtt_us.store(conn.rtt().as_micros() as u64, Relaxed);
        view.snap.store(true, Relaxed);
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Producer staging

/// A producer's per-path staging: serialized datagrams per (path, lane),
/// handed to each path's owner as ONE [`IoCmd::Send`] per lane at
/// [`TxBatch::flush`] — once per producer loop iteration. Owned by the
/// producer task (the sender's `SenderState`, a receiver local); never
/// shared. Path handles are cached and revalidated against the transport's
/// path epoch, so no map is touched per datagram.
#[derive(Default)]
pub struct TxBatch {
    slots: Vec<Slot>,
    epoch: u64,
    dirty: bool,
}

struct Slot {
    path: PathId,
    io: PathIo,
    data: Vec<Bytes>,
    ctrl: Vec<Bytes>,
}

impl TxBatch {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether anything is staged.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Stage one serialized datagram. Fails (synchronously, as quinn's own
    /// `send_datagram` did) when the path has no owner, or when the datagram
    /// exceeds the path's last published `max_datagram_size` (`TooLarge`).
    pub(crate) fn stage(
        &mut self,
        lookup: &dyn Fn(PathId) -> Option<PathIo>,
        epoch: u64,
        path: PathId,
        lane: Lane,
        b: Bytes,
    ) -> anyhow::Result<()> {
        if epoch != self.epoch {
            self.revalidate(lookup, epoch);
        }
        let i = match self.slots.iter().position(|s| s.path == path) {
            Some(i) => i,
            None => {
                let io = lookup(path)
                    .ok_or_else(|| anyhow::anyhow!("no connection on path {path}"))?;
                let data = io.view.take_vec();
                let ctrl = io.view.take_vec();
                self.slots.push(Slot { path, io, data, ctrl });
                self.slots.len() - 1
            }
        };
        let s = &mut self.slots[i];
        if let Some(max) = s.io.view.max_datagram_size() {
            if b.len() > max {
                s.io.view.ctr.too_large_staged.fetch_add(1, Relaxed);
                anyhow::bail!(
                    "datagram too large for path {path}: {} > max_datagram_size {max} (TooLarge)",
                    b.len()
                );
            }
        }
        match lane {
            Lane::Data => s.data.push(b),
            Lane::Ctrl => s.ctrl.push(b),
        }
        self.dirty = true;
        Ok(())
    }

    /// Drop cached handles of removed paths (their staged datagrams are
    /// counted as orphaned) and refresh the rest.
    fn revalidate(&mut self, lookup: &dyn Fn(PathId) -> Option<PathIo>, epoch: u64) {
        self.slots.retain_mut(|s| match lookup(s.path) {
            Some(io) => {
                s.io = io;
                true
            }
            None => {
                let n = (s.data.len() + s.ctrl.len()) as u64;
                s.io.view.ctr.orphaned.fetch_add(n, Relaxed);
                false
            }
        });
        self.epoch = epoch;
    }

    /// Hand every staged batch to its owner: data lane first, then control
    /// (a control datagram never overtakes the data staged before it). A
    /// full owner channel back-pressures here; nothing is dropped.
    pub async fn flush(&mut self) {
        if !self.dirty {
            return;
        }
        self.dirty = false;
        for s in &mut self.slots {
            for lane in [Lane::Data, Lane::Ctrl] {
                let v = match lane {
                    Lane::Data => &mut s.data,
                    Lane::Ctrl => &mut s.ctrl,
                };
                if v.is_empty() {
                    continue;
                }
                let fresh = s.io.view.take_vec();
                let dgrams = std::mem::replace(v, fresh);
                let n = dgrams.len() as u64;
                if s.io.tx.send(IoCmd::Send { lane, dgrams }).await.is_err() {
                    s.io.view.ctr.orphaned.fetch_add(n, Relaxed);
                }
            }
        }
    }
}
