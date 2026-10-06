//! Threading Q3 step 1 — the deep runtime gauges (`RWM_RTOBS=2` only).
//!
//! Measurement only: every type here delegates to what it wraps, so with the
//! gauge on the engine does the same work in the same order; with it off
//! nothing here is constructed except a pass-through wrapper.
//!
//! **Why it exists.** `[THR] os` reads CPU per OS thread, but tokio's tasks
//! migrate across the workers, so no thread reading names a task's load (the
//! Q2 rows show every worker at 0.2–0.47 core). And the `[IOWN]` asleep gauge
//! (wall − thread CPU over the owner's quinn sections) cannot separate a
//! connection-mutex wait from preemption. This module measures per task and
//! per context-switch class:
//!
//! * **`[TASK]`** — a [`Timed`] wrapper around a task's future records, per
//!   poll, wall, thread CPU and the thread's voluntary / involuntary context
//!   switches (`getrusage(RUSAGE_THREAD)`). A poll is synchronous on one
//!   thread, so the deltas are the task's own. Wall − CPU inside a poll is
//!   time asleep inside it, bucketed: `asl_vol` (a voluntary switch happened:
//!   a futex sleep — in the owner's or a driver's poll, quinn's connection
//!   mutex), `asl_inv` (only involuntary: preempted), `asl_none` (no switch
//!   was counted: hypervisor steal or clock granularity).
//! * **quinn's drivers.** quinn 0.11.9 spawns its EndpointDriver and
//!   ConnectionDriver through the endpoint's [`quinn::Runtime`]
//!   (`endpoint.rs:150`, `connection.rs:63`); [`TimedRuntime`] wraps each in
//!   a [`Timed`] (`qep-p<k>`, `qconn-p<k>`). `ConnectionDriver::poll` holds
//!   the connection mutex for the whole poll (`connection.rs:243`), so its
//!   poll wall is the lock's hold time.
//! * **`[QSOCK]`** — [`TimedSocket`] wraps the path's UDP socket: `try_send`
//!   (= `sendmsg`, called by the ConnectionDriver UNDER the connection lock)
//!   and `poll_recv` (`recvmmsg`, the EndpointDriver), calls / ns / datagrams.

use std::fmt;
use std::future::Future;
use std::io::{self, IoSliceMut};
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, OnceLock, Weak};
use std::task::{Context, Poll};
use std::time::Instant;

use quinn::udp::{RecvMeta, Transmit};

/// Whether the deep gauge is on (`RWM_RTOBS=2`).
pub fn deep() -> bool {
    crate::gates::get().rtobs_deep
}

// ───────────────────────────────────────────────────────────────────────────
// Thread clocks

/// The calling thread's CPU time (ns). Linux only.
#[cfg(target_os = "linux")]
pub fn thread_cpu_ns() -> Option<u64> {
    let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    // SAFETY: a valid out-pointer; CLOCK_THREAD_CPUTIME_ID has no other
    // precondition.
    let r = unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut ts) };
    (r == 0).then(|| ts.tv_sec as u64 * 1_000_000_000 + ts.tv_nsec as u64)
}

#[cfg(not(target_os = "linux"))]
pub fn thread_cpu_ns() -> Option<u64> {
    None
}

/// The calling thread's (voluntary, involuntary) context-switch counts.
#[cfg(target_os = "linux")]
pub fn thread_csw() -> Option<(u64, u64)> {
    // SAFETY: rusage is plain data; zeroed is a valid value.
    let mut ru: libc::rusage = unsafe { std::mem::zeroed() };
    // SAFETY: a valid out-pointer; RUSAGE_THREAD reads the calling thread.
    let r = unsafe { libc::getrusage(libc::RUSAGE_THREAD, &mut ru) };
    (r == 0).then(|| (ru.ru_nvcsw as u64, ru.ru_nivcsw as u64))
}

#[cfg(not(target_os = "linux"))]
pub fn thread_csw() -> Option<(u64, u64)> {
    None
}

/// The counters of one measured stretch class (a task's polls, or one kind
/// of owner section). Written by one task at a time; read at report time.
#[derive(Default)]
pub struct SpanCtr {
    pub n: AtomicU64,
    pub wall_ns: AtomicU64,
    pub cpu_ns: AtomicU64,
    /// Voluntary / involuntary context switches inside the stretches.
    pub vcsw: AtomicU64,
    pub ivcsw: AtomicU64,
    /// Wall − CPU of the stretches, by switch class (see the module doc).
    pub asl_vol_ns: AtomicU64,
    pub asl_inv_ns: AtomicU64,
    pub asl_none_ns: AtomicU64,
    /// Set when a clock could not be read (non-Linux): the CPU and asleep
    /// figures are then unavailable.
    pub unavailable: AtomicU64,
}

/// One measured stretch in progress.
pub struct Span {
    wall: Instant,
    cpu: Option<u64>,
    csw: Option<(u64, u64)>,
}

impl Span {
    pub fn begin() -> Self {
        Self { wall: Instant::now(), cpu: thread_cpu_ns(), csw: thread_csw() }
    }

    pub fn end(self, c: &SpanCtr) {
        let wall = self.wall.elapsed().as_nanos() as u64;
        c.n.fetch_add(1, Relaxed);
        c.wall_ns.fetch_add(wall, Relaxed);
        match (self.cpu, thread_cpu_ns(), self.csw, thread_csw()) {
            (Some(a), Some(b), Some((v0, i0)), Some((v1, i1))) => {
                let cpu = b.saturating_sub(a).min(wall);
                let (dv, di) = (v1.saturating_sub(v0), i1.saturating_sub(i0));
                c.cpu_ns.fetch_add(cpu, Relaxed);
                c.vcsw.fetch_add(dv, Relaxed);
                c.ivcsw.fetch_add(di, Relaxed);
                let asleep = wall - cpu;
                let bucket = if dv > 0 {
                    &c.asl_vol_ns
                } else if di > 0 {
                    &c.asl_inv_ns
                } else {
                    &c.asl_none_ns
                };
                bucket.fetch_add(asleep, Relaxed);
            }
            _ => {
                c.unavailable.fetch_add(1, Relaxed);
            }
        }
    }
}

/// A point-in-time copy of a [`SpanCtr`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SpanSample {
    pub n: u64,
    pub wall_ns: u64,
    pub cpu_ns: u64,
    pub vcsw: u64,
    pub ivcsw: u64,
    pub asl_vol_ns: u64,
    pub asl_inv_ns: u64,
    pub asl_none_ns: u64,
}

impl SpanCtr {
    pub fn sample(&self) -> SpanSample {
        SpanSample {
            n: self.n.load(Relaxed),
            wall_ns: self.wall_ns.load(Relaxed),
            cpu_ns: self.cpu_ns.load(Relaxed),
            vcsw: self.vcsw.load(Relaxed),
            ivcsw: self.ivcsw.load(Relaxed),
            asl_vol_ns: self.asl_vol_ns.load(Relaxed),
            asl_inv_ns: self.asl_inv_ns.load(Relaxed),
            asl_none_ns: self.asl_none_ns.load(Relaxed),
        }
    }
}

impl SpanSample {
    pub fn minus(&self, b: &SpanSample) -> SpanSample {
        let d = |x: u64, y: u64| x.saturating_sub(y);
        SpanSample {
            n: d(self.n, b.n),
            wall_ns: d(self.wall_ns, b.wall_ns),
            cpu_ns: d(self.cpu_ns, b.cpu_ns),
            vcsw: d(self.vcsw, b.vcsw),
            ivcsw: d(self.ivcsw, b.ivcsw),
            asl_vol_ns: d(self.asl_vol_ns, b.asl_vol_ns),
            asl_inv_ns: d(self.asl_inv_ns, b.asl_inv_ns),
            asl_none_ns: d(self.asl_none_ns, b.asl_none_ns),
        }
    }

    /// `n/wall_us/cpu_us/asl_vol_us/asl_inv_us/asl_none_us/vcsw/ivcsw`, the
    /// compact token value the `[IOWN]` section kinds use.
    pub fn compact(&self) -> String {
        format!(
            "{}/{}/{}/{}/{}/{}/{}/{}",
            self.n,
            self.wall_ns / 1_000,
            self.cpu_ns / 1_000,
            self.asl_vol_ns / 1_000,
            self.asl_inv_ns / 1_000,
            self.asl_none_ns / 1_000,
            self.vcsw,
            self.ivcsw
        )
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Per-task poll timing

/// One timed task's counters.
pub struct TaskCtr {
    pub serial: u64,
    pub name: String,
    pub polls: SpanCtr,
}

fn task_registry() -> &'static parking_lot::Mutex<Vec<Weak<TaskCtr>>> {
    static R: OnceLock<parking_lot::Mutex<Vec<Weak<TaskCtr>>>> = OnceLock::new();
    R.get_or_init(|| parking_lot::Mutex::new(Vec::new()))
}

/// Retired tasks' counters (a task that ended inside a window still counts).
fn retired() -> &'static parking_lot::Mutex<Vec<(u64, String, SpanSample)>> {
    static R: OnceLock<parking_lot::Mutex<Vec<(u64, String, SpanSample)>>> = OnceLock::new();
    R.get_or_init(|| parking_lot::Mutex::new(Vec::new()))
}

impl TaskCtr {
    fn new(name: String) -> Arc<Self> {
        static SERIAL: AtomicU64 = AtomicU64::new(1);
        let c = Arc::new(Self { serial: SERIAL.fetch_add(1, Relaxed), name, polls: SpanCtr::default() });
        task_registry().lock().push(Arc::downgrade(&c));
        c
    }
}

impl Drop for TaskCtr {
    fn drop(&mut self) {
        retired().lock().push((self.serial, std::mem::take(&mut self.name), self.polls.sample()));
    }
}

/// A future whose polls are timed into a named [`TaskCtr`] when the deep
/// gauge is on; a plain pass-through otherwise.
pub struct Timed<F> {
    inner: Pin<Box<F>>,
    ctr: Option<Arc<TaskCtr>>,
}

/// Wrap a task's future (`name` labels its `[TASK]` line).
pub fn timed<F: Future>(name: impl Into<String>, f: F) -> Timed<F> {
    timed_if(deep(), name, f)
}

/// [`timed`] with an explicit switch (tests).
pub fn timed_if<F: Future>(on: bool, name: impl Into<String>, f: F) -> Timed<F> {
    Timed { inner: Box::pin(f), ctr: on.then(|| TaskCtr::new(name.into())) }
}

impl<F: Future> Future for Timed<F> {
    type Output = F::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<F::Output> {
        let this = &mut *self;
        match &this.ctr {
            None => this.inner.as_mut().poll(cx),
            Some(c) => {
                let s = Span::begin();
                let r = this.inner.as_mut().poll(cx);
                s.end(&c.polls);
                r
            }
        }
    }
}

/// One task's counters at a snapshot.
#[derive(Clone, Debug, PartialEq)]
pub struct TaskSample {
    pub serial: u64,
    pub name: String,
    pub s: SpanSample,
}

/// Every timed task, live and retired.
pub fn task_samples() -> Vec<TaskSample> {
    let mut out: Vec<TaskSample> = {
        let mut r = task_registry().lock();
        r.retain(|w| w.strong_count() > 0);
        r.iter()
            .filter_map(|w| w.upgrade())
            .map(|c| TaskSample { serial: c.serial, name: c.name.clone(), s: c.polls.sample() })
            .collect()
    };
    out.extend(
        retired()
            .lock()
            .iter()
            .map(|(serial, name, s)| TaskSample { serial: *serial, name: name.clone(), s: *s }),
    );
    out
}

/// Render the `[TASK]` lines for the window `from → to` (joined by serial;
/// a task with no poll in the window is skipped). `cores` = CPU / window
/// wall; `busy` = poll wall / window wall. Pure, so the token set is tested.
pub fn task_lines(phase: &str, side: &str, extra: &str, wall_s: f64, from: &[TaskSample], to: &[TaskSample]) -> Vec<String> {
    let wall_s = wall_s.max(1e-9);
    let mut v: Vec<(String, String)> = to
        .iter()
        .filter_map(|t| {
            let b = from.iter().find(|f| f.serial == t.serial).map(|f| f.s).unwrap_or_default();
            let d = t.s.minus(&b);
            (d.n > 0).then(|| {
                let line = format!(
                    "[TASK] phase={phase} side={side}{extra} task={} polls={} cpu_us={} wall_us={} \
                     cores={:.3} busy={:.3} asl_vol_us={} asl_inv_us={} asl_none_us={} vcsw={} ivcsw={} \
                     cpu_per_poll_us={:.2} wall_s={wall_s:.3}",
                    t.name,
                    d.n,
                    d.cpu_ns / 1_000,
                    d.wall_ns / 1_000,
                    d.cpu_ns as f64 / 1e9 / wall_s,
                    d.wall_ns as f64 / 1e9 / wall_s,
                    d.asl_vol_ns / 1_000,
                    d.asl_inv_ns / 1_000,
                    d.asl_none_ns / 1_000,
                    d.vcsw,
                    d.ivcsw,
                    d.cpu_ns as f64 / 1e3 / d.n as f64,
                );
                (t.name.clone(), line)
            })
        })
        .collect();
    v.sort_by(|a, b| a.0.cmp(&b.0));
    v.into_iter().map(|(_, l)| l).collect()
}

// ───────────────────────────────────────────────────────────────────────────
// quinn's runtime and socket, timed

/// The socket counters of one path (`[QSOCK]`).
#[derive(Default)]
pub struct SockCtr {
    /// `try_send` calls (one `sendmsg` each; GSO batches several datagrams).
    pub send_calls: AtomicU64,
    pub send_ns: AtomicU64,
    /// Datagrams those calls carried (`contents / segment_size`, rounded up).
    pub send_dgrams: AtomicU64,
    /// Calls that returned `WouldBlock`.
    pub send_wouldblock: AtomicU64,
    /// `poll_recv` calls that returned datagrams, their ns and datagrams.
    pub recv_calls: AtomicU64,
    pub recv_ns: AtomicU64,
    pub recv_dgrams: AtomicU64,
}

/// One path's socket counters at a snapshot.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SockSample {
    pub serial: u64,
    pub label: String,
    pub v: [u64; 7],
}

pub struct SockEntry {
    pub serial: u64,
    pub label: String,
    pub ctr: SockCtr,
}

fn sock_registry() -> &'static parking_lot::Mutex<Vec<Arc<SockEntry>>> {
    static R: OnceLock<parking_lot::Mutex<Vec<Arc<SockEntry>>>> = OnceLock::new();
    R.get_or_init(|| parking_lot::Mutex::new(Vec::new()))
}

fn new_sock_entry(label: String) -> Arc<SockEntry> {
    static SERIAL: AtomicU64 = AtomicU64::new(1);
    let e = Arc::new(SockEntry { serial: SERIAL.fetch_add(1, Relaxed), label, ctr: SockCtr::default() });
    sock_registry().lock().push(e.clone());
    e
}

pub fn sock_samples() -> Vec<SockSample> {
    sock_registry()
        .lock()
        .iter()
        .map(|e| {
            let c = &e.ctr;
            SockSample {
                serial: e.serial,
                label: e.label.clone(),
                v: [
                    c.send_calls.load(Relaxed),
                    c.send_ns.load(Relaxed),
                    c.send_dgrams.load(Relaxed),
                    c.send_wouldblock.load(Relaxed),
                    c.recv_calls.load(Relaxed),
                    c.recv_ns.load(Relaxed),
                    c.recv_dgrams.load(Relaxed),
                ],
            }
        })
        .collect()
}

/// Render the `[QSOCK]` lines for the window `from → to`.
pub fn sock_lines(phase: &str, side: &str, extra: &str, wall_s: f64, from: &[SockSample], to: &[SockSample]) -> Vec<String> {
    let wall_s = wall_s.max(1e-9);
    to.iter()
        .map(|t| {
            let b = from.iter().find(|f| f.serial == t.serial).map(|f| f.v).unwrap_or_default();
            let d: Vec<u64> = t.v.iter().zip(b.iter()).map(|(x, y)| x.saturating_sub(*y)).collect();
            format!(
                "[QSOCK] phase={phase} side={side}{extra} sock={} send_calls={} send_us={} send_dg={} \
                 send_wb={} gso={:.2} send_us_per_call={:.2} send_frac={:.4} recv_calls={} recv_us={} recv_dg={} \
                 recv_frac={:.4} wall_s={wall_s:.3}",
                t.label,
                d[0],
                d[1] / 1_000,
                d[2],
                d[3],
                d[2] as f64 / d[0].max(1) as f64,
                d[1] as f64 / 1e3 / d[0].max(1) as f64,
                d[1] as f64 / 1e9 / wall_s,
                d[4],
                d[5] / 1_000,
                d[6],
                d[5] as f64 / 1e9 / wall_s,
            )
        })
        .collect()
}

/// A delegating [`quinn::Runtime`]: the drivers it spawns are [`Timed`] and
/// the socket it wraps is a [`TimedSocket`]. The first spawn is the
/// EndpointDriver (`Endpoint::new`), every later one a ConnectionDriver.
pub struct TimedRuntime {
    inner: Arc<dyn quinn::Runtime>,
    label: String,
    spawned: AtomicU64,
}

impl TimedRuntime {
    pub fn new(inner: Arc<dyn quinn::Runtime>, label: String) -> Self {
        Self { inner, label, spawned: AtomicU64::new(0) }
    }
}

impl fmt::Debug for TimedRuntime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TimedRuntime").field("label", &self.label).finish()
    }
}

impl quinn::Runtime for TimedRuntime {
    fn new_timer(&self, i: Instant) -> Pin<Box<dyn quinn::AsyncTimer>> {
        self.inner.new_timer(i)
    }

    fn spawn(&self, future: Pin<Box<dyn Future<Output = ()> + Send>>) {
        let kind = if self.spawned.fetch_add(1, Relaxed) == 0 { "qep" } else { "qconn" };
        self.inner.spawn(Box::pin(timed_if(true, format!("{kind}-{}", self.label), future)));
    }

    fn wrap_udp_socket(&self, t: std::net::UdpSocket) -> io::Result<Arc<dyn quinn::AsyncUdpSocket>> {
        let inner = self.inner.wrap_udp_socket(t)?;
        Ok(Arc::new(TimedSocket { inner, e: new_sock_entry(self.label.clone()) }))
    }

    fn now(&self) -> Instant {
        self.inner.now()
    }
}

/// A delegating UDP socket that times `try_send` and `poll_recv`.
pub struct TimedSocket {
    inner: Arc<dyn quinn::AsyncUdpSocket>,
    e: Arc<SockEntry>,
}

impl fmt::Debug for TimedSocket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

impl quinn::AsyncUdpSocket for TimedSocket {
    fn create_io_poller(self: Arc<Self>) -> Pin<Box<dyn quinn::UdpPoller>> {
        self.inner.clone().create_io_poller()
    }

    fn try_send(&self, transmit: &Transmit) -> io::Result<()> {
        let t0 = Instant::now();
        let r = self.inner.try_send(transmit);
        let c = &self.e.ctr;
        c.send_ns.fetch_add(t0.elapsed().as_nanos() as u64, Relaxed);
        c.send_calls.fetch_add(1, Relaxed);
        match &r {
            Ok(()) => {
                let n = match transmit.segment_size {
                    Some(s) if s > 0 => transmit.contents.len().div_ceil(s),
                    _ => 1,
                };
                c.send_dgrams.fetch_add(n as u64, Relaxed);
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                c.send_wouldblock.fetch_add(1, Relaxed);
            }
            Err(_) => {}
        }
        r
    }

    fn poll_recv(&self, cx: &mut Context, bufs: &mut [IoSliceMut<'_>], meta: &mut [RecvMeta]) -> Poll<io::Result<usize>> {
        let t0 = Instant::now();
        let r = self.inner.poll_recv(cx, bufs, meta);
        if let Poll::Ready(Ok(n)) = &r {
            let c = &self.e.ctr;
            c.recv_ns.fetch_add(t0.elapsed().as_nanos() as u64, Relaxed);
            c.recv_calls.fetch_add(1, Relaxed);
            let dg: usize = meta[..*n]
                .iter()
                .map(|m| if m.stride > 0 { m.len.div_ceil(m.stride) } else { 1 })
                .sum();
            c.recv_dgrams.fetch_add(dg as u64, Relaxed);
        }
        r
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.inner.local_addr()
    }

    fn max_transmit_segments(&self) -> usize {
        self.inner.max_transmit_segments()
    }

    fn max_receive_segments(&self) -> usize {
        self.inner.max_receive_segments()
    }

    fn may_fragment(&self) -> bool {
        self.inner.may_fragment()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_task_line_carries_window_deltas_and_skips_idle_tasks() {
        let s = |n, wall, cpu| SpanSample { n, wall_ns: wall, cpu_ns: cpu, ..Default::default() };
        let from = vec![
            TaskSample { serial: 1, name: "sender".into(), s: s(10, 1_000_000, 500_000) },
            TaskSample { serial: 2, name: "idle".into(), s: s(5, 10, 10) },
        ];
        let mut to = from.clone();
        to[0].s = SpanSample { asl_vol_ns: 200_000, vcsw: 3, ..s(110, 501_000_000, 400_500_000) };
        let l = task_lines("xfer", "client", " obj=1", 2.0, &from, &to);
        assert_eq!(l.len(), 1, "{l:?}");
        assert!(
            l[0].starts_with(
                "[TASK] phase=xfer side=client obj=1 task=sender polls=100 cpu_us=400000 wall_us=500000 \
                 cores=0.200 busy=0.250 asl_vol_us=200 "
            ),
            "{}",
            l[0]
        );
        assert!(l[0].contains(" vcsw=3 ") && l[0].contains(" cpu_per_poll_us=4000.00 "), "{}", l[0]);
    }

    #[test]
    fn a_timed_future_records_its_polls_and_returns_the_inner_output() {
        let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
        let f = timed_if(true, "t", async {
            tokio::task::yield_now().await;
            7
        });
        let c = f.ctr.clone().unwrap();
        assert_eq!(rt.block_on(f), 7);
        let s = c.polls.sample();
        assert_eq!(s.n, 2, "one Pending poll, one Ready poll");
        #[cfg(target_os = "linux")]
        assert!(s.cpu_ns > 0 && s.cpu_ns <= s.wall_ns, "{s:?}");
    }

    #[test]
    fn an_untimed_future_has_no_counter() {
        let f = timed_if(false, "t", async { 1 });
        assert!(f.ctr.is_none());
    }

    #[test]
    fn the_sock_line_reports_gso_and_per_call_cost() {
        let to = vec![SockSample { serial: 1, label: "client-p0".into(), v: [100, 2_000_000, 900, 0, 50, 500_000, 1000] }];
        let l = sock_lines("xfer", "client", "", 1.0, &[], &to);
        assert!(l[0].contains(" sock=client-p0 send_calls=100 send_us=2000 send_dg=900 send_wb=0 gso=9.00 send_us_per_call=20.00 "), "{}", l[0]);
    }
}
