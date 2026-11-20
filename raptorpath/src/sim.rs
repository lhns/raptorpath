//! Deterministic simulation network under quinn (ADR-0082, test-detection
//! plan S8; feasibility in `docs/derivations/sim-spike.md`). `--features sim`
//! only; never in the release binary.
//!
//! [`SimRuntime`] wraps quinn's tokio runtime. Its `wrap_udp_socket` keeps
//! the real bound socket alive (never polled) and returns a [`SimSocket`] on
//! a deterministic virtual address: a configured server address, or
//! 127.0.0.1:50000+n for each client bind in bind order. Each direction is
//! one FIFO link: serialization at `rate_bps`, a fixed one-way `delay`, a
//! seeded jitter, a drop-tail queue and seeded Bernoulli loss (both streams
//! keyed by the seed and the direction, not by the server's real port), plus an
//! optional client-to-server blackout. Every send (with its fate) and every
//! receive goes into the trace as `virt_us TX|RX dir src>dst ord len fate`.
//!
//! **Determinism.** The runtime is current-thread with paused time, so time
//! advances only when every task is idle; tokio's `select!` order is seeded
//! ([`runtime`], which needs `--cfg tokio_unstable`); the engine clock and
//! RNG (`crate::clock`) follow the paused clock and one seed; quinn's RNG is
//! seeded per endpoint ([`endpoint_config`]); per-path maps are ordered.
//! Process-global statics (the clock's RNG, `net::now_us`'s base, the test
//! seams) make it one scenario per process.
//!
//! **Time resolution: 1 ms.** tokio's timer wheel has 1 ms slots and the
//! paused clock auto-advances to the next timer deadline, so every virtual
//! timestamp is a multiple of 1 ms. Sub-ms serialization and pacing collapse
//! into 1 ms bursts. Jitter therefore needs an amplitude of several ms to
//! show up as RTT variance; with jitter off, a fixed-delay link has zero RTT
//! variance (the spike measured srtt exactly 22 000 µs, sigma 0).
//!
//! **Jitter does not reorder.** A datagram's delivery time is clamped to be
//! no earlier than the previous one's on the same direction (FIFO), so
//! jitter moves RTT samples but never reorders the link. The jitter draws
//! come from their own stream, so turning jitter on does not change which
//! datagrams the Bernoulli loss drops.

// `cfg(tokio_unstable)` is set only by sim builds (RUSTFLAGS), so rustc's
// check-cfg does not know it.
#![allow(unexpected_cfgs)]

use quinn::udp::{RecvMeta, Transmit};
use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::future::Future;
use std::io::{self, IoSliceMut};
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::time::Duration;
use tokio::time::{Instant, Sleep};

/// Per-datagram delay jitter, added to the fixed one-way delay.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Jitter {
    None,
    /// Uniform in [0, max).
    Uniform { max: Duration },
    /// Normal with this standard deviation, truncated so the total one-way
    /// delay never goes below zero.
    Normal { sigma: Duration },
}

/// One direction's link parameters.
#[derive(Clone, Debug)]
pub struct LinkCfg {
    pub rate_bps: f64,
    pub delay: Duration,
    pub jitter: Jitter,
    /// Bernoulli loss probability per datagram.
    pub loss: f64,
    pub queue_bytes: usize,
}

impl LinkCfg {
    /// 40 Mbit/s, 10 ms one-way, uniform 0-4 ms jitter, 500 kB queue.
    pub fn default_40m(loss: f64) -> Self {
        LinkCfg {
            rate_bps: 40e6,
            delay: Duration::from_millis(10),
            jitter: Jitter::Uniform { max: Duration::from_millis(4) },
            loss,
            queue_bytes: 500_000,
        }
    }
}

/// Drop every client-to-server datagram from its `start_dgram`-th one
/// (counting only datagrams of at least `min_len` bytes) for `dur`.
#[derive(Clone, Debug)]
pub struct Blackout {
    pub start_dgram: u64,
    pub min_len: usize,
    pub dur: Duration,
}

pub struct SimNet {
    t0: Instant,
    seed: u64,
    st: Mutex<NetState>,
    trace: Mutex<Vec<String>>,
    /// Virtual µs of the last event (read by a wall-clock watchdog thread).
    pub last_virt_us: AtomicU64,
    pub events: AtomicU64,
}

struct Dir {
    cfg: LinkCfg,
    next_free: Instant,
    last_at: Instant,
    loss_rng: u64,
    jitter_rng: u64,
    ord: u64,
    big: u64,
    black_until: Option<Instant>,
}

struct NetState {
    socks: BTreeMap<SocketAddr, Arc<SockQ>>,
    servers: Vec<SocketAddr>,
    next_port: u16,
    c2s: LinkCfg,
    s2c: LinkCfg,
    dirs: BTreeMap<(SocketAddr, SocketAddr), Dir>,
    blackout: Option<Blackout>,
}

#[derive(Default)]
struct SockQ {
    q: Mutex<VecDeque<(Instant, SocketAddr, Vec<u8>, u64)>>,
    waker: Mutex<Option<Waker>>,
    sleep: Mutex<Option<Pin<Box<Sleep>>>>,
}

/// SplitMix64's output mix (also the engine RNG's, `crate::clock`).
pub fn splitmix_mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn next_u64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    splitmix_mix(*state)
}

fn uniform(state: &mut u64) -> f64 {
    (next_u64(state) >> 11) as f64 / (1u64 << 53) as f64
}

fn jitter_us(j: Jitter, state: &mut u64) -> f64 {
    match j {
        Jitter::None => 0.0,
        Jitter::Uniform { max } => uniform(state) * max.as_micros() as f64,
        Jitter::Normal { sigma } => {
            // Box-Muller; u1 in (0, 1].
            let u1 = 1.0 - uniform(state);
            let u2 = uniform(state);
            (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos() * sigma.as_micros() as f64
        }
    }
}

static NET: Mutex<Option<Arc<SimNet>>> = Mutex::new(None);

impl SimNet {
    /// Install the network and seed the engine clock's RNG. Must be called
    /// inside the paused runtime, before any endpoint binds.
    pub fn install(seed: u64, servers: Vec<SocketAddr>, c2s: LinkCfg, s2c: LinkCfg, blackout: Option<Blackout>) -> Arc<SimNet> {
        crate::clock::activate_sim(seed);
        let net = Arc::new(SimNet {
            t0: Instant::now(),
            seed,
            st: Mutex::new(NetState {
                socks: BTreeMap::new(),
                servers,
                next_port: 50_000,
                c2s,
                s2c,
                dirs: BTreeMap::new(),
                blackout,
            }),
            trace: Mutex::new(Vec::new()),
            last_virt_us: AtomicU64::new(0),
            events: AtomicU64::new(0),
        });
        *NET.lock().unwrap() = Some(net.clone());
        net
    }

    /// The trace so far, one event per line.
    pub fn trace(&self) -> String {
        let t = self.trace.lock().unwrap();
        let mut s = t.join("\n");
        s.push('\n');
        s
    }

    fn us(&self, t: Instant) -> u64 {
        t.saturating_duration_since(self.t0).as_micros() as u64
    }

    fn log(&self, line: String) {
        self.events.fetch_add(1, Relaxed);
        self.trace.lock().unwrap().push(line);
    }

    fn send(&self, src: SocketAddr, dst: SocketAddr, data: &[u8]) {
        let now = Instant::now();
        let tus = self.us(now);
        self.last_virt_us.store(tus, Relaxed);
        let mut st = self.st.lock().unwrap();
        let to_server = st.servers.contains(&dst);
        let cfg = if to_server { st.c2s.clone() } else { st.s2c.clone() };
        // Stream key per direction: a server by its index, a client by its
        // virtual port. Never the server's real port: that is a harness
        // choice, and keying on it redrew every loss and jitter sample.
        let key = |a: &SocketAddr| st.servers.iter().position(|s| s == a).map_or(a.port() as u64, |i| i as u64);
        let base = self.seed ^ (key(&src) << 16) ^ key(&dst);
        let blackout = st.blackout.clone();
        let dq = st.socks.get(&dst).cloned();
        let d = st.dirs.entry((src, dst)).or_insert_with(|| Dir {
            cfg,
            next_free: now,
            last_at: now,
            loss_rng: splitmix_mix(base),
            jitter_rng: splitmix_mix(base ^ 0x6A09_E667_F3BC_C908),
            ord: 0,
            big: 0,
            black_until: None,
        });
        d.ord += 1;
        let ord = d.ord;
        let tag = if to_server { "c>s" } else { "s>c" };
        let mut black_start = None;
        if to_server {
            if let Some(b) = &blackout {
                if data.len() >= b.min_len {
                    d.big += 1;
                    if d.big == b.start_dgram && d.black_until.is_none() {
                        d.black_until = Some(now + b.dur);
                        black_start = Some(b.dur);
                    }
                }
            }
        }
        // Draw both streams for every datagram, so each stream's sequence
        // depends only on the datagram count, not on the other's outcomes.
        let lost = uniform(&mut d.loss_rng) < d.cfg.loss;
        let jit = jitter_us(d.cfg.jitter, &mut d.jitter_rng);
        let fate = if d.black_until.is_some_and(|u| now < u) {
            "drop_black"
        } else if lost {
            "drop_loss"
        } else {
            let backlog = d.next_free.saturating_duration_since(now);
            let qbytes = (backlog.as_secs_f64() * d.cfg.rate_bps / 8.0) as usize;
            if qbytes + data.len() > d.cfg.queue_bytes {
                "drop_queue"
            } else {
                let ser = Duration::from_secs_f64(data.len() as f64 * 8.0 / d.cfg.rate_bps);
                d.next_free = d.next_free.max(now) + ser;
                let one_way = (d.cfg.delay.as_micros() as f64 + jit).max(0.0);
                let at = (d.next_free + Duration::from_micros(one_way as u64)).max(d.last_at);
                d.last_at = at;
                if let Some(q) = &dq {
                    q.q.lock().unwrap().push_back((at, src, data.to_vec(), ord));
                    if let Some(w) = q.waker.lock().unwrap().take() {
                        w.wake();
                    }
                    "ok"
                } else {
                    "drop_nosock"
                }
            }
        };
        drop(st);
        if let Some(dur) = black_start {
            self.log(format!("{tus} BLACKOUT_START {tag} ord={ord} dur_ms={}", dur.as_millis()));
        }
        self.log(format!("{tus} TX {tag} {}>{} ord={ord} len={} {fate}", src.port(), dst.port(), data.len()));
    }
}

/// The installed network, if any.
pub fn net() -> Option<Arc<SimNet>> {
    NET.lock().unwrap().clone()
}

/// `crate::clock::endpoint_config` under `sim`: with a network installed,
/// put the endpoint on it and seed quinn's RNG from the scenario seed.
pub(crate) fn endpoint_config(
    runtime: &mut Arc<dyn quinn::Runtime>,
    role: &str,
    path_id: crate::scheduler::PathId,
) -> quinn::EndpointConfig {
    let mut cfg = quinn::EndpointConfig::default();
    if let Some(net) = net() {
        *runtime = Arc::new(SimRuntime(runtime.clone()));
        let mut s = net.seed ^ ((path_id as u64) << 8) ^ if role == "server" { 0x5a } else { 0xa5 };
        let mut seed = [0u8; 32];
        for c in seed.chunks_mut(8) {
            c.copy_from_slice(&next_u64(&mut s).to_le_bytes());
        }
        cfg.rng_seed(Some(seed));
    }
    cfg
}

/// The simulation runtime: current-thread, paused time, `select!` order
/// seeded. Needs `--cfg tokio_unstable` (tokio's `Builder::rng_seed`); sim
/// builds set it in `RUSTFLAGS`, never the release build.
#[cfg(tokio_unstable)]
pub fn runtime(seed: u64) -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .start_paused(true)
        .rng_seed(tokio::runtime::RngSeed::from_bytes(&seed.to_le_bytes()))
        .build()
        .expect("sim runtime")
}

#[derive(Debug)]
pub struct SimRuntime(pub Arc<dyn quinn::Runtime>);

impl quinn::Runtime for SimRuntime {
    fn new_timer(&self, i: std::time::Instant) -> Pin<Box<dyn quinn::AsyncTimer>> {
        self.0.new_timer(i)
    }
    fn spawn(&self, future: Pin<Box<dyn Future<Output = ()> + Send>>) {
        self.0.spawn(future)
    }
    fn wrap_udp_socket(&self, t: std::net::UdpSocket) -> io::Result<Arc<dyn quinn::AsyncUdpSocket>> {
        let net = net().ok_or_else(|| io::Error::other("no sim net"))?;
        let real = t.local_addr()?;
        let mut st = net.st.lock().unwrap();
        let addr = if st.servers.contains(&real) {
            real
        } else {
            let p = st.next_port;
            st.next_port += 1;
            SocketAddr::from(([127, 0, 0, 1], p))
        };
        let q = Arc::new(SockQ::default());
        st.socks.insert(addr, q.clone());
        drop(st);
        net.log(format!("0 BIND {addr}"));
        Ok(Arc::new(SimSocket { net, addr, q, _real: t }))
    }
    fn now(&self) -> std::time::Instant {
        self.0.now()
    }
}

pub struct SimSocket {
    net: Arc<SimNet>,
    addr: SocketAddr,
    q: Arc<SockQ>,
    _real: std::net::UdpSocket,
}

impl fmt::Debug for SimSocket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SimSocket({})", self.addr)
    }
}

#[derive(Debug)]
struct Ready;
impl quinn::UdpPoller for Ready {
    fn poll_writable(self: Pin<&mut Self>, _cx: &mut Context) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

impl quinn::AsyncUdpSocket for SimSocket {
    fn create_io_poller(self: Arc<Self>) -> Pin<Box<dyn quinn::UdpPoller>> {
        Box::pin(Ready)
    }

    fn try_send(&self, t: &Transmit) -> io::Result<()> {
        match t.segment_size {
            Some(s) if s > 0 && s < t.contents.len() => {
                for c in t.contents.chunks(s) {
                    self.net.send(self.addr, t.destination, c);
                }
            }
            _ => self.net.send(self.addr, t.destination, t.contents),
        }
        Ok(())
    }

    fn poll_recv(&self, cx: &mut Context, bufs: &mut [IoSliceMut<'_>], meta: &mut [RecvMeta]) -> Poll<io::Result<usize>> {
        loop {
            let now = Instant::now();
            let mut got = Vec::new();
            let head;
            {
                let mut q = self.q.q.lock().unwrap();
                while got.len() < bufs.len().min(meta.len()) {
                    match q.front() {
                        Some((at, ..)) if *at <= now => got.push(q.pop_front().unwrap()),
                        _ => break,
                    }
                }
                head = q.front().map(|e| e.0);
            }
            if !got.is_empty() {
                let tus = self.net.us(now);
                self.net.last_virt_us.store(tus, Relaxed);
                let n = got.len();
                for (i, (_, src, data, ord)) in got.into_iter().enumerate() {
                    let len = data.len().min(bufs[i].len());
                    bufs[i][..len].copy_from_slice(&data[..len]);
                    let mut m = RecvMeta::default();
                    m.addr = src;
                    m.len = len;
                    m.stride = len;
                    m.dst_ip = Some(self.addr.ip());
                    meta[i] = m;
                    let tag = if src.port() >= 50_000 { "c>s" } else { "s>c" };
                    self.net.log(format!("{tus} RX {tag} {}>{} ord={ord} len={len}", src.port(), self.addr.port()));
                }
                return Poll::Ready(Ok(n));
            }
            *self.q.waker.lock().unwrap() = Some(cx.waker().clone());
            let Some(at) = head else { return Poll::Pending };
            let mut sl = self.q.sleep.lock().unwrap();
            match sl.as_mut() {
                Some(s) => s.as_mut().reset(at),
                None => *sl = Some(Box::pin(tokio::time::sleep_until(at))),
            }
            if sl.as_mut().unwrap().as_mut().poll(cx).is_pending() {
                return Poll::Pending;
            }
            // The head is due now: loop and take it.
        }
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.addr)
    }
}
