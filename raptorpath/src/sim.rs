//! SIM SPIKE (throwaway, branch `sim-spike`): an in-memory UDP network under
//! quinn, on tokio's paused clock. A [`SimRuntime`] wraps quinn's tokio
//! runtime; its `wrap_udp_socket` ignores the real socket (kept alive, never
//! polled) and returns a [`SimSocket`] on a deterministic virtual address.
//! Each direction is one FIFO link: serialization at `rate`, fixed one-way
//! `delay`, a drop-tail queue cap, seeded Bernoulli loss, and an optional
//! blackout window. Every send / drop / receive is appended to the trace.

use quinn::udp::{RecvMeta, Transmit};
use std::collections::{HashMap, VecDeque};
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

/// One direction's link parameters.
#[derive(Clone, Debug)]
pub struct LinkCfg {
    pub rate_bps: f64,
    pub delay: Duration,
    pub loss: f64,
    pub queue_bytes: usize,
}

/// Drop every datagram on the client->server direction from its `start_dgram`-th
/// datagram (counting only datagrams of at least `min_len` bytes) for `dur`.
#[derive(Clone, Debug)]
pub struct Blackout {
    pub start_dgram: u64,
    pub min_len: usize,
    pub dur: Duration,
}

pub struct SimNet {
    t0: Instant,
    st: Mutex<NetState>,
    pub trace: Mutex<Vec<String>>,
    /// Virtual µs of the last event (read by the wall-clock watchdog thread).
    pub last_virt_us: AtomicU64,
    pub events: AtomicU64,
}

struct Dir {
    cfg: LinkCfg,
    next_free: Instant,
    rng: u64,
    ord: u64,
    big: u64,
    black_until: Option<Instant>,
}

struct NetState {
    socks: HashMap<SocketAddr, Arc<SockQ>>,
    servers: Vec<SocketAddr>,
    next_port: u16,
    seed: u64,
    c2s: LinkCfg,
    s2c: LinkCfg,
    dirs: HashMap<(SocketAddr, SocketAddr), Dir>,
    blackout: Option<Blackout>,
}

#[derive(Default)]
struct SockQ {
    q: Mutex<VecDeque<(Instant, SocketAddr, Vec<u8>, u64)>>,
    waker: Mutex<Option<Waker>>,
    sleep: Mutex<Option<Pin<Box<Sleep>>>>,
}

fn splitmix(state: &mut u64) -> f64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 11) as f64 / (1u64 << 53) as f64
}

static NET: Mutex<Option<Arc<SimNet>>> = Mutex::new(None);

impl SimNet {
    /// Must be called inside the paused runtime.
    pub fn install(seed: u64, servers: Vec<SocketAddr>, c2s: LinkCfg, s2c: LinkCfg, blackout: Option<Blackout>) -> Arc<SimNet> {
        let net = Arc::new(SimNet {
            t0: Instant::now(),
            st: Mutex::new(NetState {
                socks: HashMap::new(),
                servers,
                next_port: 50_000,
                seed,
                c2s,
                s2c,
                dirs: HashMap::new(),
                blackout,
            }),
            trace: Mutex::new(Vec::new()),
            last_virt_us: AtomicU64::new(0),
            events: AtomicU64::new(0),
        });
        *NET.lock().unwrap() = Some(net.clone());
        net
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
        let seed = st.seed ^ ((src.port() as u64) << 16) ^ dst.port() as u64;
        let blackout = st.blackout.clone();
        let dq = st.socks.get(&dst).cloned();
        let d = st.dirs.entry((src, dst)).or_insert_with(|| Dir {
            cfg,
            next_free: now,
            rng: seed,
            ord: 0,
            big: 0,
            black_until: None,
        });
        d.ord += 1;
        let ord = d.ord;
        let tag = if to_server { "c>s" } else { "s>c" };
        if to_server {
            if let Some(b) = &blackout {
                if data.len() >= b.min_len {
                    d.big += 1;
                    if d.big == b.start_dgram && d.black_until.is_none() {
                        d.black_until = Some(now + b.dur);
                        drop(st);
                        self.log(format!("{tus} BLACKOUT_START {tag} ord={ord} dur_ms={}", b.dur.as_millis()));
                        st = self.st.lock().unwrap();
                    }
                }
            }
        }
        let d = st.dirs.get_mut(&(src, dst)).unwrap();
        let fate = if d.black_until.is_some_and(|u| now < u) {
            "drop_black"
        } else if splitmix(&mut d.rng) < d.cfg.loss {
            "drop_loss"
        } else {
            let backlog = d.next_free.saturating_duration_since(now);
            let qbytes = (backlog.as_secs_f64() * d.cfg.rate_bps / 8.0) as usize;
            if qbytes + data.len() > d.cfg.queue_bytes {
                "drop_queue"
            } else {
                let ser = Duration::from_secs_f64(data.len() as f64 * 8.0 / d.cfg.rate_bps);
                d.next_free = d.next_free.max(now) + ser;
                let at = d.next_free + d.cfg.delay;
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
        self.log(format!("{tus} TX {tag} {}>{} ord={ord} len={} {fate}", src.port(), dst.port(), data.len()));
    }
}

/// The installed network, if any (the `bind_endpoint` hook reads this).
pub fn net() -> Option<Arc<SimNet>> {
    NET.lock().unwrap().clone()
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
