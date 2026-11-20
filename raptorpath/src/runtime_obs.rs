//! Runtime observability (threading redesign P0): the `[THR]` per-worker and
//! per-thread CPU readout and the `[LAG]` scheduling-lag probe.
//!
//! Measurement only. The process runtime is the one `#[tokio::main]` built
//! (`Builder::new_multi_thread().enable_all()`, default worker count = the
//! available parallelism); the only builder change is `thread_name_fn`, so
//! the OS can attribute CPU per thread (`rp-w-<n>`). No other runtime knob is
//! touched.
//!
//! **`[THR] rt`** — per tokio worker, from the STABLE `RuntimeMetrics` only
//! (no `tokio_unstable`): `worker_total_busy_duration`, `worker_park_count`,
//! `worker_park_unpark_count`, plus `num_workers`. One line per worker.
//!
//! **`[THR] os`** — per OS thread of the process (Linux: utime + stime of
//! `/proc/self/task/<tid>/stat`, `comm` = the thread name truncated to 15
//! bytes), the main (`block_on`) thread included. One line per thread. Not
//! available elsewhere (`[THR] os unavailable`). `rp-w-<n>` is the n-th
//! thread the runtime's pool spawned: the workers are launched first, in
//! index order, so `rp-w-<i>` for `i < num_workers` is worker `i` — a
//! hypothesis from tokio's launch order (`worker_thread_id` is unstable, so
//! the join cannot be proven with stable metrics); `n ≥ num_workers` are
//! blocking-pool threads (none expected on the Linux engine path).
//!
//! **`[LAG]`** — one task on the runtime ticks every [`LAG_TICK`] (tokio
//! `interval`, `MissedTickBehavior::Delay`) and records how late each tick
//! was woken (`now − scheduled`): the Cats-Effect-style starvation probe
//! (th1 §1.5, CE's `cpuStarvationCheck`, here at a 10 ms tick so a 2 ms-RTT
//! cell's starvation is visible). Instrument floor: tokio rounds timer
//! deadlines UP to the next millisecond, so every tick reads 0–1 ms late by
//! construction; p99 / max are the signal, not p50. The probe runs on a
//! worker, so it does not see the `block_on` main thread.
//!
//! **Opt-in** (`RWM_RTOBS=1`, threading P2a step 0): unarmed, neither the
//! probe task nor the `/proc` reads exist; the thread names stay.
//!
//! Phases: `phase=xfer` lines bracket one perf object (the perf client's
//! timed run, the perf server's object from its first packet to its
//! completion; [`snapshot`] at the start, [`emit_window`] at the end), so per-thread
//! cores are CPU over the transfer's own wall; `phase=run` lines are emitted
//! once at process end, cumulative since the runtime was built.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use parking_lot::Mutex;

/// The lag probe's tick (provenance: th1 §1.5, the CE-style starvation
/// probe; status §11 P0 pre-registration).
pub const LAG_TICK: Duration = Duration::from_millis(10);

/// Sample cap for the lag log: 2^20 ticks of 10 ms ≈ 2.9 h of run, far past
/// any battery invocation; beyond it samples are dropped (counted).
const LAG_MAX_SAMPLES: usize = 1 << 20;

/// The runtime's thread-name function: `rp-w-<n>`, `n` = spawn order.
pub fn worker_thread_name() -> String {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    format!("rp-w-{}", NEXT.fetch_add(1, Ordering::Relaxed))
}

/// The process runtime: exactly what `#[tokio::main]` builds, plus thread
/// names.
pub fn build_runtime() -> std::io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name_fn(worker_thread_name)
        .build()
}

/// One lag sample: when the tick was observed (µs since the probe's origin)
/// and how late it was (µs).
#[derive(Clone, Copy, Debug, PartialEq)]
struct LagSample {
    at_us: u64,
    late_us: u64,
}

/// The lag probe's sample log.
#[derive(Default)]
pub struct LagLog {
    samples: Mutex<Vec<LagSample>>,
    dropped: AtomicUsize,
}

impl LagLog {
    fn push(&self, s: LagSample) {
        let mut v = self.samples.lock();
        if v.len() < LAG_MAX_SAMPLES {
            v.push(s);
        } else {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Every sample, in order.
    fn all(&self) -> Vec<LagSample> {
        self.samples.lock().clone()
    }

    /// The lateness samples (µs) observed in `[from_us, to_us]`.
    fn window(&self, from_us: u64, to_us: u64) -> Vec<u64> {
        self.samples
            .lock()
            .iter()
            .filter(|s| s.at_us >= from_us && s.at_us <= to_us)
            .map(|s| s.late_us)
            .collect()
    }
}

/// THE quantile rule (tools/l1/l1common.py `q`): linear interpolation between
/// closest ranks, h = (n−1)·p. `None` on an empty sample.
pub fn quantile(xs: &[u64], p: f64) -> Option<f64> {
    if xs.is_empty() {
        return None;
    }
    let mut v: Vec<u64> = xs.to_vec();
    v.sort_unstable();
    let h = (v.len() - 1) as f64 * p.clamp(0.0, 1.0);
    let lo = h.floor() as usize;
    let hi = (lo + 1).min(v.len() - 1);
    Some(v[lo] as f64 + (h - lo as f64) * (v[hi] as f64 - v[lo] as f64))
}

/// Render one `[LAG]` line from a window's lateness samples.
pub fn lag_line(phase: &str, side: &str, extra: &str, late_us: &[u64], dropped: usize) -> String {
    let f = |p: f64| quantile(late_us, p).map_or_else(|| "-".to_string(), |x| format!("{x:.0}"));
    format!(
        "[LAG] phase={phase} side={side}{extra} tick_ms={} n={} p50_us={} p99_us={} max_us={} min_us={} dropped={dropped}",
        LAG_TICK.as_millis(),
        late_us.len(),
        f(0.5),
        f(0.99),
        late_us.iter().max().map_or_else(|| "-".to_string(), |m| m.to_string()),
        late_us.iter().min().map_or_else(|| "-".to_string(), |m| m.to_string()),
    )
}

// ───────────────────────────────────────────────────────────────────────────
// The OS reference clock (`RWM_RTOBS=2`, status §26 (a))
//
// The `[LAG]` probe ticks on the tokio timer driver, so its lateness mixes
// the runtime (timer-wheel rounding, a worker that is busy or parked) with
// the OS (CPU contention, hypervisor). One plain OS thread — outside the
// runtime — sleeps on CLOCK_MONOTONIC to absolute 10 ms deadlines
// (`clock_nanosleep(TIMER_ABSTIME)`, the kernel's own hrtimer, no rounding
// to a millisecond) and logs how late each wake was. If the `[LAG]` tail is
// the OS/VM, this clock sees it too; if it is the runtime, it does not.
//
// Time base. The deadlines and `late_us` are on CLOCK_MONOTONIC. `at_us` is
// relative to the SAME `origin` the lag probe uses: at arm time
// `origin = Instant::now()` (CLOCK_MONOTONIC on Linux) and, right next to
// it, `origin_mono_ns` = `clock_gettime(CLOCK_MONOTONIC)`; then
// `at_us = (mono_now − origin_mono_ns) / 1000`. The two origin reads are
// ~100 ns apart, a constant offset far below the 1 ms timeline resolution.

/// One OS-clock sample: when the wake was observed (µs since the probe's
/// origin), how late it was (µs), and how many whole deadlines were skipped
/// just before it (the thread overslept more than a tick).
#[derive(Clone, Copy, Debug, PartialEq)]
struct OsLagSample {
    at_us: u64,
    late_us: u64,
    skipped: u64,
}

/// The OS reference clock's sample log (same capped-Vec pattern as [`LagLog`]).
#[derive(Default)]
pub struct OsLagLog {
    samples: Mutex<Vec<OsLagSample>>,
    dropped: AtomicUsize,
}

impl OsLagLog {
    fn push(&self, s: OsLagSample) {
        let mut v = self.samples.lock();
        if v.len() < LAG_MAX_SAMPLES {
            v.push(s);
        } else {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn all(&self) -> Vec<OsLagSample> {
        self.samples.lock().clone()
    }

    /// The lateness samples (µs) observed in `[from_us, to_us]` and the
    /// deadlines skipped before them.
    fn window(&self, from_us: u64, to_us: u64) -> (Vec<u64>, u64) {
        let v = self.samples.lock();
        let hit = v.iter().filter(|s| s.at_us >= from_us && s.at_us <= to_us);
        let (mut late, mut skipped) = (Vec::new(), 0);
        for s in hit {
            late.push(s.late_us);
            skipped += s.skipped;
        }
        (late, skipped)
    }
}

/// The deadline after `deadline` (one tick on) given the clock reads `now`:
/// if that is already in the past, whole ticks are skipped until it is not
/// (`next >= now`). Returns `(next, skipped)`. Pure.
fn advance_deadline(deadline_ns: u64, now_ns: u64, tick_ns: u64) -> (u64, u64) {
    let next = deadline_ns + tick_ns;
    if next >= now_ns {
        return (next, 0);
    }
    let k = (now_ns - next - 1) / tick_ns + 1;
    (next + k * tick_ns, k)
}

/// Render one `[OSLAG]` line from a window's lateness samples.
pub fn os_lag_line(phase: &str, side: &str, extra: &str, late_us: &[u64], skipped: u64) -> String {
    let f = |p: f64| quantile(late_us, p).map_or_else(|| "-".to_string(), |x| format!("{x:.0}"));
    format!(
        "[OSLAG] phase={phase} side={side}{extra} n={} p50_us={} p99_us={} max_us={} min_us={} skipped={skipped}",
        late_us.len(),
        f(0.5),
        f(0.99),
        late_us.iter().max().map_or_else(|| "-".to_string(), |m| m.to_string()),
        late_us.iter().min().map_or_else(|| "-".to_string(), |m| m.to_string()),
    )
}

/// Render the `[OSLAGRAW]` line (same format and cap as `[LAGRAW]`).
fn os_lag_raw_line(side: &str, origin_unix_us: u64, xfer: Option<(u64, u64)>, samples: &[OsLagSample], cap: usize) -> String {
    raw_line("OSLAGRAW", side, origin_unix_us, xfer, samples.iter().map(|s| (s.at_us, s.late_us)), cap)
}

/// `run_delay` (ns) from a `/proc/<pid>/task/<tid>/schedstat` record
/// (`<cpu time ns> <run-queue wait ns> <timeslices>`): field 2, the time the
/// thread spent runnable but not running.
pub fn parse_schedstat(s: &str) -> Option<u64> {
    s.split_whitespace().nth(1)?.parse().ok()
}

/// The `[LAGRAW]` / `[OSLAGRAW]` line body: `n` samples of which the first
/// `cap` are printed as `at_us:late_us`.
fn raw_line(
    tag: &str,
    side: &str,
    origin_unix_us: u64,
    xfer: Option<(u64, u64)>,
    samples: impl ExactSizeIterator<Item = (u64, u64)>,
    cap: usize,
) -> String {
    let n = samples.len();
    let shown: Vec<String> = samples.take(cap).map(|(a, l)| format!("{a}:{l}")).collect();
    let xf = xfer.map_or_else(|| "-".to_string(), |(a, b)| format!("{a}-{b}"));
    let s = if shown.is_empty() { "-".to_string() } else { shown.join(",") };
    format!("[{tag}] side={side} origin_unix_us={origin_unix_us} xfer={xf} n={n} printed={} s={s}", shown.len())
}

/// CLOCK_MONOTONIC now (ns). `std::time::Instant` is this clock on Linux, so
/// reading it right next to the lag probe's `origin` gives the OS thread a
/// timeline on the same axis (see the module section above).
#[cfg(target_os = "linux")]
fn mono_clock_ns() -> u64 {
    let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    // SAFETY: a valid out-pointer; CLOCK_MONOTONIC has no other precondition.
    unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) };
    ts.tv_sec as u64 * 1_000_000_000 + ts.tv_nsec as u64
}

/// The reference thread's body: sleep to absolute `LAG_TICK` deadlines on
/// the tick grid anchored at `origin_mono_ns` (the first boundary after the
/// start), log each wake's lateness, until `stop`.
#[cfg(target_os = "linux")]
fn os_ref_loop(log: &OsLagLog, origin_mono_ns: u64, stop: &std::sync::atomic::AtomicBool) {
    let tick = LAG_TICK.as_nanos() as u64;
    let now = mono_clock_ns();
    let mut deadline = origin_mono_ns + (now.saturating_sub(origin_mono_ns) / tick + 1) * tick;
    let mut skipped = 0u64;
    while !stop.load(Ordering::Relaxed) {
        let ts = libc::timespec {
            tv_sec: (deadline / 1_000_000_000) as _,
            tv_nsec: (deadline % 1_000_000_000) as _,
        };
        // SAFETY: a valid timespec; the remaining-time pointer is unused with
        // TIMER_ABSTIME. Returns the error number (EINTR on a signal), not -1.
        let rc = unsafe { libc::clock_nanosleep(libc::CLOCK_MONOTONIC, libc::TIMER_ABSTIME, &ts, std::ptr::null_mut()) };
        if rc == libc::EINTR {
            continue; // same absolute deadline again
        }
        if rc != 0 {
            return; // not a clock we can use: stay silent rather than spin
        }
        let now = mono_clock_ns();
        log.push(OsLagSample {
            at_us: now.saturating_sub(origin_mono_ns) / 1_000,
            late_us: now.saturating_sub(deadline) / 1_000,
            skipped,
        });
        (deadline, skipped) = advance_deadline(deadline, now, tick);
    }
}

/// Start the `rp-osref` thread (a plain OS thread, outside the runtime).
#[cfg(target_os = "linux")]
fn spawn_os_ref(
    log: Arc<OsLagLog>,
    origin_mono_ns: u64,
    stop: Arc<std::sync::atomic::AtomicBool>,
) -> std::io::Result<std::thread::JoinHandle<()>> {
    std::thread::Builder::new()
        .name("rp-osref".into())
        .spawn(move || os_ref_loop(&log, origin_mono_ns, &stop))
}

/// Cap on printed `[LAGRAW]` samples (20000 ticks of 10 ms = 200 s).
const LAGRAW_PRINT_CAP: usize = 20_000;

/// Render the `[LAGRAW]` line (`RWM_RTOBS=2`, once per process at run end):
/// the probe's origin on the wall clock, the last transfer window's bounds
/// (µs since the origin) and every sample as `at_us:late_us`, so a late tick
/// can be placed on the unix timeline next to the `[LPOLL]` long polls.
/// Pure, so the token set is unit-tested.
fn lag_raw_line(side: &str, origin_unix_us: u64, xfer: Option<(u64, u64)>, samples: &[LagSample], cap: usize) -> String {
    raw_line("LAGRAW", side, origin_unix_us, xfer, samples.iter().map(|s| (s.at_us, s.late_us)), cap)
}

/// The lag probe task body.
async fn lag_probe(log: Arc<LagLog>, origin: Instant) {
    let mut iv = tokio::time::interval(LAG_TICK);
    iv.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        let scheduled = iv.tick().await;
        let now = tokio::time::Instant::now();
        let late = now.saturating_duration_since(scheduled);
        let at = now.into_std().saturating_duration_since(origin);
        log.push(LagSample { at_us: at.as_micros() as u64, late_us: late.as_micros() as u64 });
    }
}

/// One tokio worker's stable counters.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WorkerSample {
    pub busy: Duration,
    pub park: u64,
    pub unpark: u64,
}

/// One OS thread's CPU (clock ticks, utime + stime).
#[derive(Clone, Debug, PartialEq)]
pub struct ThreadSample {
    pub tid: u32,
    pub comm: String,
    pub ticks: u64,
    /// `run_delay` (ns, `/proc/.../schedstat` field 2) under `RWM_RTOBS=2`:
    /// `None` = not collected (the `[THR] os` line is then unchanged),
    /// `Some(None)` = collected but unreadable, `Some(Some(ns))` = read.
    pub delay_ns: Option<Option<u64>>,
}

/// A point-in-time reading of every counter `[THR]` prints.
#[derive(Clone, Debug)]
pub struct Snapshot {
    at: Instant,
    workers: Vec<WorkerSample>,
    /// `None` where the OS reader is unavailable (non-Linux).
    threads: Option<Vec<ThreadSample>>,
    /// Threading Q1: every live I/O owner's counters.
    owners: Vec<OwnerSample>,
    /// Threading Q3 step 1 (`RWM_RTOBS=2`; empty otherwise): every timed
    /// task's and every timed quinn socket's counters.
    tasks: Vec<crate::task_obs::TaskSample>,
    socks: Vec<crate::task_obs::SockSample>,
    /// Threading R1 (`RWM_RTOBS=2`; empty otherwise): every receiver task's
    /// section CPU and backlog.
    rx: Vec<crate::task_obs::RxSecSample>,
    /// Sender diagnosis (`RWM_RTOBS=2`; empty otherwise): the sender loop's
    /// section counters.
    tx: Vec<crate::task_obs::TxSecSample>,
}

/// Parse one `/proc/<pid>/task/<tid>/stat` record: `(comm, utime + stime)`.
/// `comm` sits in parentheses and may itself contain spaces or `)`, so the
/// fields are counted from the LAST `)`; after it, field 3 (state) is index
/// 0, so utime (field 14) is index 11 and stime (field 15) index 12. Spaces
/// in `comm` render as `_` so the token stays whitespace-free.
pub fn parse_task_stat(stat: &str) -> Option<(String, u64)> {
    let open = stat.find('(')?;
    let close = stat.rfind(')')?;
    if close < open {
        return None;
    }
    let comm = stat[open + 1..close].replace(char::is_whitespace, "_");
    let rest: Vec<&str> = stat[close + 1..].split_whitespace().collect();
    let utime: u64 = rest.get(11)?.parse().ok()?;
    let stime: u64 = rest.get(12)?.parse().ok()?;
    Some((comm, utime + stime))
}

#[cfg(target_os = "linux")]
fn read_threads(with_delay: bool) -> Option<Vec<ThreadSample>> {
    let mut out = Vec::new();
    for e in std::fs::read_dir("/proc/self/task").ok()?.flatten() {
        let Some(tid) = e.file_name().to_str().and_then(|s| s.parse::<u32>().ok()) else {
            continue;
        };
        // A thread that exited between the listing and the read is skipped.
        let Ok(stat) = std::fs::read_to_string(e.path().join("stat")) else {
            continue;
        };
        if let Some((comm, ticks)) = parse_task_stat(&stat) {
            // The thread's `schedstat` (CONFIG_SCHED_INFO): one more small
            // /proc read per thread, only under the deep gauge.
            let delay_ns = with_delay.then(|| {
                std::fs::read_to_string(e.path().join("schedstat")).ok().and_then(|s| parse_schedstat(&s))
            });
            out.push(ThreadSample { tid, comm, ticks, delay_ns });
        }
    }
    out.sort_by_key(|t| t.tid);
    Some(out)
}

#[cfg(not(target_os = "linux"))]
fn read_threads(_with_delay: bool) -> Option<Vec<ThreadSample>> {
    None
}

#[cfg(target_os = "linux")]
fn clk_tck() -> f64 {
    // SAFETY: sysconf has no preconditions.
    let t = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    if t > 0 {
        t as f64
    } else {
        100.0
    }
}

#[cfg(not(target_os = "linux"))]
fn clk_tck() -> f64 {
    100.0
}

/// Render the `[THR]` lines for the interval `from → to` (`from = None`:
/// cumulative since process start, with `wall` the runtime's age). Pure, so
/// the token set is unit-tested.
pub fn thr_lines(
    phase: &str,
    side: &str,
    extra: &str,
    wall: Duration,
    from: Option<(&[WorkerSample], Option<&[ThreadSample]>)>,
    to: (&[WorkerSample], Option<&[ThreadSample]>),
    hz: f64,
) -> Vec<String> {
    let wall_s = wall.as_secs_f64().max(1e-9);
    let mut out = Vec::new();
    let (w_to, t_to) = to;
    let w_from = from.map(|f| f.0);
    let mut busy_sum = 0.0;
    for (i, w) in w_to.iter().enumerate() {
        let base = w_from.and_then(|f| f.get(i)).copied().unwrap_or_default();
        let busy = w.busy.saturating_sub(base.busy).as_secs_f64();
        busy_sum += busy;
        out.push(format!(
            "[THR] rt phase={phase} side={side}{extra} worker={i} busy_s={busy:.3} busy_frac={:.3} park={} unpark={} wall_s={wall_s:.3}",
            busy / wall_s,
            w.park.saturating_sub(base.park),
            w.unpark.saturating_sub(base.unpark),
        ));
    }
    match t_to {
        None => out.push(format!(
            "[THR] os unavailable phase={phase} side={side}{extra} (per-thread CPU is read from /proc on Linux only)"
        )),
        Some(ts) => {
            let t_from = from.and_then(|f| f.1);
            let mut cpu_sum = 0.0;
            for t in ts {
                let base = t_from
                    .and_then(|f| f.iter().find(|b| b.tid == t.tid))
                    .map_or(0, |b| b.ticks);
                let cpu = t.ticks.saturating_sub(base) as f64 / hz;
                cpu_sum += cpu;
                // `run_delay_us` (RWM_RTOBS=2): the thread's time runnable
                // but not running over the window; `-` if either end is
                // unreadable. A thread born in the window counts from 0.
                let delay = t.delay_ns.map_or_else(String::new, |now| {
                    let base = match t_from.and_then(|f| f.iter().find(|b| b.tid == t.tid)) {
                        None => Some(0),
                        Some(b) => b.delay_ns.flatten(),
                    };
                    match (now, base) {
                        (Some(n), Some(b)) => format!(" run_delay_us={}", n.saturating_sub(b) / 1_000),
                        _ => " run_delay_us=-".to_string(),
                    }
                });
                out.push(format!(
                    "[THR] os phase={phase} side={side}{extra} tid={} comm={} cpu_s={cpu:.3} cores={:.3} wall_s={wall_s:.3}{delay}",
                    t.tid,
                    t.comm,
                    cpu / wall_s,
                ));
            }
            out.push(format!(
                "[THR] sum phase={phase} side={side}{extra} workers={} busy_s={busy_sum:.3} threads={} cpu_s={cpu_sum:.3} cores={:.3} wall_s={wall_s:.3}",
                w_to.len(),
                ts.len(),
                cpu_sum / wall_s,
            ));
            return out;
        }
    }
    out.push(format!(
        "[THR] sum phase={phase} side={side}{extra} workers={} busy_s={busy_sum:.3} threads=- cpu_s=- cores=- wall_s={wall_s:.3}",
        w_to.len(),
    ));
    out
}

struct Obs {
    handle: tokio::runtime::Handle,
    origin: Instant,
    /// `origin` on the wall clock (µs since the UNIX epoch), read right next
    /// to it, so `[LAGRAW]` samples can be joined to `[LPOLL]` end times.
    origin_unix_us: u64,
    lag: Arc<LagLog>,
    /// The OS reference clock's log (`RWM_RTOBS=2`, Linux; empty otherwise).
    oslag: Arc<OsLagLog>,
    side: OnceLock<&'static str>,
    /// The last `emit_window`'s bounds (µs since `origin`), for `[LAGRAW]`.
    xfer: Mutex<Option<(u64, u64)>>,
}

/// One I/O owner's counters at a snapshot (the `[IOWN]` window join key is
/// `serial`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OwnerSample {
    pub serial: u64,
    pub path: u32,
    pub rt: String,
    pub polls: u64,
    pub drains: u64,
    pub tx_batches: u64,
    pub tx_dgrams: u64,
    pub ctrl_batches: u64,
    pub ctrl_dgrams: u64,
    pub rx_batches: u64,
    pub rx_dgrams: u64,
    pub rx_capped: u64,
    pub ack_batches: u64,
    pub ack_dgrams: u64,
    pub send_err: u64,
    pub too_large_staged: u64,
    pub orphaned: u64,
    pub qcalls: u64,
    pub q_secs: u64,
    pub q_wall_ns: u64,
    pub q_cpu_ns: u64,
    pub q_cpu_unavailable: bool,
    pub drv_on: u64,
    pub drv_off: u64,
    /// Threading Q3 step 1 (`RWM_RTOBS=2`; zeros otherwise).
    pub k_send: crate::task_obs::SpanSample,
    pub k_read: crate::task_obs::SpanSample,
    pub k_pub: crate::task_obs::SpanSample,
    pub rx_full: u64,
    pub rx_full_ns: u64,
    pub ack_full: u64,
    pub ack_full_ns: u64,
    pub fl_sends: u64,
    pub fl_full: u64,
    pub fl_full_ns: u64,
    /// Threading R1: the inbound-`Vec` pool's hits and misses (always
    /// counted), and the owner command channel's depth at each drain
    /// (`RWM_RTOBS=2` only).
    pub ib_hit: u64,
    pub ib_miss: u64,
    pub cmdq: crate::task_obs::DepthSample,
}

fn owner_samples() -> Vec<OwnerSample> {
    use std::sync::atomic::Ordering::Relaxed;
    crate::transport::io_owner::live_views()
        .iter()
        .map(|v| {
            let c = &v.ctr;
            OwnerSample {
                serial: v.serial,
                path: v.path,
                rt: v.rt.clone(),
                polls: c.polls.load(Relaxed),
                drains: c.drains.load(Relaxed),
                tx_batches: c.tx_batches.load(Relaxed),
                tx_dgrams: c.tx_dgrams.load(Relaxed),
                ctrl_batches: c.ctrl_batches.load(Relaxed),
                ctrl_dgrams: c.ctrl_dgrams.load(Relaxed),
                rx_batches: c.rx_batches.load(Relaxed),
                rx_dgrams: c.rx_dgrams.load(Relaxed),
                rx_capped: c.rx_capped.load(Relaxed),
                ack_batches: c.ack_batches.load(Relaxed),
                ack_dgrams: c.ack_dgrams.load(Relaxed),
                send_err: c.send_err.load(Relaxed),
                too_large_staged: c.too_large_staged.load(Relaxed),
                orphaned: c.orphaned.load(Relaxed),
                qcalls: c.qcalls.load(Relaxed),
                q_secs: c.q_secs.load(Relaxed),
                q_wall_ns: c.q_wall_ns.load(Relaxed),
                q_cpu_ns: c.q_cpu_ns.load(Relaxed),
                q_cpu_unavailable: c.q_cpu_unavailable.load(Relaxed),
                drv_on: v.drv_on.load(Relaxed),
                drv_off: v.drv_off.load(Relaxed),
                k_send: c.k_send.sample(),
                k_read: c.k_read.sample(),
                k_pub: c.k_pub.sample(),
                rx_full: c.rx_full.load(Relaxed),
                rx_full_ns: c.rx_full_ns.load(Relaxed),
                ack_full: c.ack_full.load(Relaxed),
                ack_full_ns: c.ack_full_ns.load(Relaxed),
                fl_sends: c.fl_sends.load(Relaxed),
                fl_full: c.fl_full.load(Relaxed),
                fl_full_ns: c.fl_full_ns.load(Relaxed),
                ib_hit: c.ib_hit.load(Relaxed),
                ib_miss: c.ib_miss.load(Relaxed),
                cmdq: c.cmdq.sample(),
            }
        })
        .collect()
}

/// Render the `[IOWN]` lines (one per I/O owner) for the interval
/// `from → to` (`from` empty: cumulative). `q_asleep_us` = the owner's wall
/// time asleep inside its quinn sections (section wall − thread CPU): the
/// connection-mutex wait plus any involuntary preemption there;
/// `asleep_frac` = that over the window's wall. Pure, so the token set is
/// unit-tested.
///
/// Threading Q3 step 1 appends (zeros unless `RWM_RTOBS=2`): `qk_snd=`,
/// `qk_rd=`, `qk_pub=` — the quinn sections by kind as
/// `n/wall_us/cpu_us/asl_vol_us/asl_inv_us/asl_none_us/vcsw/ivcsw`
/// (`task_obs::SpanSample::compact`); `rx_full=`/`rx_full_us=`,
/// `ack_full=`/`ack_full_us=` (inbound batches that found the receiver's /
/// sender's channel full, and the wall until forwarded); `fl_sends=`,
/// `fl_full=`, `fl_full_us=` (producer batches handed to this owner, those
/// that found its channel full, and the producers' wait).
///
/// Threading R1 appends `ibp_hit=`/`ibp_miss=` (the inbound-`Vec` pool; always
/// counted) and `cmdq_n=`/`cmdq_p50=`/`cmdq_p99=`/`cmdq_max=` (the owner
/// command channel's depth at each drain, `RWM_RTOBS=2`; `-` when empty;
/// the max is run-cumulative).
pub fn iown_lines(phase: &str, side: &str, extra: &str, wall: Duration, from: &[OwnerSample], to: &[OwnerSample]) -> Vec<String> {
    let wall_s = wall.as_secs_f64().max(1e-9);
    to.iter()
        .map(|t| {
            let b = from.iter().find(|f| f.serial == t.serial).cloned().unwrap_or_default();
            let d = |x: u64, y: u64| x.saturating_sub(y);
            let q_wall_ns = d(t.q_wall_ns, b.q_wall_ns);
            let (cq_n, cq_p50, cq_p99, cq_max) = t.cmdq.minus(&b.cmdq).tokens();
            let (asleep, frac) = if t.q_cpu_unavailable || t.q_secs == 0 {
                ("-".to_string(), "-".to_string())
            } else {
                let asleep_ns = q_wall_ns.saturating_sub(d(t.q_cpu_ns, b.q_cpu_ns));
                (
                    format!("{}", asleep_ns / 1_000),
                    format!("{:.4}", asleep_ns as f64 / 1e9 / wall_s),
                )
            };
            format!(
                "[IOWN] phase={phase} side={side}{extra} path={} rt={} polls={} drains={} \
                 tx_batches={} tx_dg={} ctrl_batches={} ctrl_dg={} rx_batches={} rx_dg={} rx_capped={} \
                 ack_batches={} ack_dg={} send_err={} too_large_staged={} orphaned={} qcalls={} q_secs={} q_wall_us={} q_asleep_us={asleep} \
                 asleep_frac={frac} drv_on={} drv_off={} wall_s={wall_s:.3} \
                 qk_snd={} qk_rd={} qk_pub={} rx_full={} rx_full_us={} ack_full={} ack_full_us={} \
                 fl_sends={} fl_full={} fl_full_us={} ibp_hit={} ibp_miss={} cmdq_n={} cmdq_p50={} cmdq_p99={} cmdq_max={}",
                t.path,
                t.rt,
                d(t.polls, b.polls),
                d(t.drains, b.drains),
                d(t.tx_batches, b.tx_batches),
                d(t.tx_dgrams, b.tx_dgrams),
                d(t.ctrl_batches, b.ctrl_batches),
                d(t.ctrl_dgrams, b.ctrl_dgrams),
                d(t.rx_batches, b.rx_batches),
                d(t.rx_dgrams, b.rx_dgrams),
                d(t.rx_capped, b.rx_capped),
                d(t.ack_batches, b.ack_batches),
                d(t.ack_dgrams, b.ack_dgrams),
                d(t.send_err, b.send_err),
                d(t.too_large_staged, b.too_large_staged),
                d(t.orphaned, b.orphaned),
                d(t.qcalls, b.qcalls),
                d(t.q_secs, b.q_secs),
                q_wall_ns / 1_000,
                d(t.drv_on, b.drv_on),
                d(t.drv_off, b.drv_off),
                t.k_send.minus(&b.k_send).compact(),
                t.k_read.minus(&b.k_read).compact(),
                t.k_pub.minus(&b.k_pub).compact(),
                d(t.rx_full, b.rx_full),
                d(t.rx_full_ns, b.rx_full_ns) / 1_000,
                d(t.ack_full, b.ack_full),
                d(t.ack_full_ns, b.ack_full_ns) / 1_000,
                d(t.fl_sends, b.fl_sends),
                d(t.fl_full, b.fl_full),
                d(t.fl_full_ns, b.fl_full_ns) / 1_000,
                d(t.ib_hit, b.ib_hit),
                d(t.ib_miss, b.ib_miss),
                cq_n,
                cq_p50,
                cq_p99,
                cq_max,
            )
        })
        .collect()
}

static OBS: OnceLock<Obs> = OnceLock::new();

/// Arm the observer on the current runtime: spawn the lag probe and keep the
/// handle for the metrics. Idempotent; called once by `main` for the `run`
/// and `perf` commands. Must be called from inside the runtime.
///
/// Opt-in: a no-op unless `RWM_RTOBS=1` (`gates.rs`, echoed on `[GATES]`).
/// Status §9 finding 6: the always-on instrument (the 100 Hz probe task,
/// the per-window `/proc` reads) cost the c1s goodput; unarmed, no probe
/// task exists, [`snapshot`] returns `None` and no `[THR]`/`[LAG]` line is
/// printed. The thread names ([`build_runtime`]) are unconditional.
pub fn arm() {
    if !crate::gates::get().rtobs {
        return;
    }
    arm_unconditionally();
}

/// Whether the OS reference clock runs (and prints) in this process: the deep
/// gauge on Linux; nothing elsewhere.
fn os_ref_active() -> bool {
    cfg!(target_os = "linux") && crate::task_obs::deep()
}

/// Whether the observer is armed in this process.
pub fn armed() -> bool {
    OBS.get().is_some()
}

/// The armed path of [`arm`].
fn arm_unconditionally() {
    let handle = tokio::runtime::Handle::current();
    let origin = Instant::now();
    // Read next to `origin`: the OS reference thread's `at_us` axis.
    #[cfg(target_os = "linux")]
    let origin_mono_ns = mono_clock_ns();
    let origin_unix_us = crate::task_obs::unix_us();
    let lag = Arc::new(LagLog::default());
    let oslag = Arc::new(OsLagLog::default());
    if OBS
        .set(Obs {
            handle: handle.clone(),
            origin,
            origin_unix_us,
            lag: lag.clone(),
            oslag: oslag.clone(),
            side: OnceLock::new(),
            xfer: Mutex::new(None),
        })
        .is_ok()
    {
        handle.spawn(lag_probe(lag, origin));
        // The OS reference clock runs for the life of the process (the stop
        // flag is only for the unit test).
        #[cfg(target_os = "linux")]
        if crate::task_obs::deep() {
            let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
            if let Err(e) = spawn_os_ref(oslag, origin_mono_ns, stop) {
                tracing::warn!("rp-osref thread not started: {e}");
            }
        }
    }
}

/// Label this process's lines (`client` / `server`); default `run`.
pub fn set_side(side: &'static str) {
    if let Some(o) = OBS.get() {
        let _ = o.side.set(side);
    }
}

fn side_of(o: &Obs) -> &'static str {
    o.side.get().copied().unwrap_or("run")
}

/// Read every counter now. `None` when the observer is not armed.
pub fn snapshot() -> Option<Snapshot> {
    let o = OBS.get()?;
    let m = o.handle.metrics();
    let workers = (0..m.num_workers())
        .map(|i| WorkerSample {
            busy: m.worker_total_busy_duration(i),
            park: m.worker_park_count(i),
            unpark: m.worker_park_unpark_count(i),
        })
        .collect();
    Some(Snapshot {
        at: Instant::now(),
        workers,
        threads: read_threads(crate::task_obs::deep()),
        owners: owner_samples(),
        tasks: crate::task_obs::task_samples(),
        socks: crate::task_obs::sock_samples(),
        rx: crate::task_obs::rxsec_samples(),
        tx: if crate::task_obs::deep() { crate::task_obs::txsec_samples() } else { Vec::new() },
    })
}

/// The `[IOWN]` lines of every I/O owner for the window `from → to`.
fn io_lines(phase: &str, side: &str, extra: &str, wall: Duration, from: Option<&Snapshot>, to: &Snapshot) -> Vec<String> {
    let base: &[OwnerSample] = from.map_or(&[], |f| f.owners.as_slice());
    let mut v = iown_lines(phase, side, extra, wall, base, &to.owners);
    // Threading Q3 step 1: the deep gauge's lines (none unless RWM_RTOBS=2).
    let tb: &[crate::task_obs::TaskSample] = from.map_or(&[], |f| f.tasks.as_slice());
    v.extend(crate::task_obs::task_lines(phase, side, extra, wall.as_secs_f64(), tb, &to.tasks));
    let sb: &[crate::task_obs::SockSample] = from.map_or(&[], |f| f.socks.as_slice());
    v.extend(crate::task_obs::sock_lines(phase, side, extra, wall.as_secs_f64(), sb, &to.socks));
    let rb: &[crate::task_obs::RxSecSample] = from.map_or(&[], |f| f.rx.as_slice());
    v.extend(crate::task_obs::rxsec_lines(phase, side, extra, wall.as_secs_f64(), rb, &to.rx));
    let xb: &[crate::task_obs::TxSecSample] = from.map_or(&[], |f| f.tx.as_slice());
    v.extend(crate::task_obs::txsec_lines(phase, side, extra, wall.as_secs_f64(), xb, &to.tx));
    v
}

/// Print the `[THR]` and `[LAG]` lines for the window `from → now`
/// (`phase=xfer`); `extra` is appended to the line head (` obj=1`).
pub fn emit_window(from: &Snapshot, extra: &str) {
    let (Some(o), Some(to)) = (OBS.get(), snapshot()) else {
        return;
    };
    let side = side_of(o);
    let wall = to.at.saturating_duration_since(from.at);
    for l in thr_lines(
        "xfer",
        side,
        extra,
        wall,
        Some((from.workers.as_slice(), from.threads.as_deref())),
        (to.workers.as_slice(), to.threads.as_deref()),
        clk_tck(),
    ) {
        crate::readout!("{l}");
    }
    let a = from.at.saturating_duration_since(o.origin).as_micros() as u64;
    let b = to.at.saturating_duration_since(o.origin).as_micros() as u64;
    *o.xfer.lock() = Some((a, b));
    let lat = o.lag.window(a, b);
    crate::readout!("{}", lag_line("xfer", side, extra, &lat, o.lag.dropped.load(Ordering::Relaxed)));
    if os_ref_active() {
        let (late, skipped) = o.oslag.window(a, b);
        crate::readout!("{}", os_lag_line("xfer", side, extra, &late, skipped));
    }
    for l in io_lines("xfer", side, extra, wall, Some(from), &to) {
        crate::readout!("{l}");
    }
}

/// Print the cumulative `phase=run` lines (process end). No-op when the
/// observer is not armed.
pub fn emit_run_end() {
    let (Some(o), Some(to)) = (OBS.get(), snapshot()) else {
        return;
    };
    let side = side_of(o);
    let wall = to.at.saturating_duration_since(o.origin);
    for l in thr_lines(
        "run",
        side,
        "",
        wall,
        None,
        (to.workers.as_slice(), to.threads.as_deref()),
        clk_tck(),
    ) {
        crate::readout!("{l}");
    }
    let lat = o.lag.window(0, u64::MAX);
    crate::readout!(
        "{} final=1",
        lag_line("run", side, "", &lat, o.lag.dropped.load(Ordering::Relaxed))
    );
    if os_ref_active() {
        let (late, skipped) = o.oslag.window(0, u64::MAX);
        crate::readout!("{}", os_lag_line("run", side, "", &late, skipped));
    }
    for l in io_lines("run", side, "", wall, None, &to) {
        crate::readout!("{l}");
    }
    // Lag diagnosis (RWM_RTOBS=2): the raw ticks and the long polls.
    if crate::task_obs::deep() {
        let xfer = *o.xfer.lock();
        crate::readout!("{}", lag_raw_line(side, o.origin_unix_us, xfer, &o.lag.all(), LAGRAW_PRINT_CAP));
        if os_ref_active() {
            crate::readout!("{}", os_lag_raw_line(side, o.origin_unix_us, xfer, &o.oslag.all(), LAGRAW_PRINT_CAP));
        }
        let (polls, dropped) = crate::task_obs::long_polls();
        for l in crate::task_obs::lpoll_lines(side, &polls, dropped, crate::task_obs::LPOLL_PRINT_CAP) {
            crate::readout!("{l}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_task_stat_parser_reads_comm_and_cpu_from_the_last_paren() {
        // A real /proc/<pid>/task/<tid>/stat record (fields 1..20 shown).
        let s = "4242 (rp-w-3) S 1 4242 4242 0 -1 4194368 1234 0 0 0 517 83 0 0 20 0 9 0";
        assert_eq!(parse_task_stat(s), Some(("rp-w-3".to_string(), 600)));
        // comm with a space and a ')' inside: counted from the LAST ')'.
        let s = "7 (a b) c) R 1 7 7 0 -1 0 0 0 0 0 10 5 0 0 20 0 1 0";
        assert_eq!(parse_task_stat(s), Some(("a_b)_c".to_string(), 15)));
        assert_eq!(parse_task_stat("garbage"), None);
        assert_eq!(parse_task_stat("1 (x) S 1 2"), None);
    }

    #[test]
    fn the_quantile_rule_is_l1commons() {
        // Linear interpolation between closest ranks (numpy default).
        assert_eq!(quantile(&[], 0.5), None);
        assert_eq!(quantile(&[5], 0.99), Some(5.0));
        assert_eq!(quantile(&[1, 2, 3, 4], 0.5), Some(2.5));
        assert_eq!(quantile(&[4, 1, 3, 2], 0.5), Some(2.5));
        let v: Vec<u64> = (0..=100).collect();
        assert_eq!(quantile(&v, 0.99), Some(99.0));
    }

    #[test]
    fn the_thr_lines_carry_deltas_and_cores_over_the_window() {
        let from_w = [WorkerSample { busy: Duration::from_millis(100), park: 10, unpark: 8 }];
        let to_w = [
            WorkerSample { busy: Duration::from_millis(1100), park: 30, unpark: 25 },
        ];
        let from_t = [ThreadSample { tid: 11, comm: "rp-w-0".into(), ticks: 50, delay_ns: None }];
        let to_t = [
            ThreadSample { tid: 11, comm: "rp-w-0".into(), ticks: 150, delay_ns: None },
            // A thread born inside the window counts from 0.
            ThreadSample { tid: 12, comm: "raptorpath".into(), ticks: 40, delay_ns: None },
        ];
        let l = thr_lines(
            "xfer",
            "server",
            " obj=1",
            Duration::from_secs(2),
            Some((&from_w[..], Some(&from_t[..]))),
            (&to_w[..], Some(&to_t[..])),
            100.0,
        );
        assert_eq!(l.len(), 4, "{l:?}");
        assert_eq!(
            l[0],
            "[THR] rt phase=xfer side=server obj=1 worker=0 busy_s=1.000 busy_frac=0.500 park=20 unpark=17 wall_s=2.000"
        );
        assert_eq!(
            l[1],
            "[THR] os phase=xfer side=server obj=1 tid=11 comm=rp-w-0 cpu_s=1.000 cores=0.500 wall_s=2.000"
        );
        assert_eq!(
            l[2],
            "[THR] os phase=xfer side=server obj=1 tid=12 comm=raptorpath cpu_s=0.400 cores=0.200 wall_s=2.000"
        );
        assert_eq!(
            l[3],
            "[THR] sum phase=xfer side=server obj=1 workers=1 busy_s=1.000 threads=2 cpu_s=1.400 cores=0.700 wall_s=2.000"
        );
    }

    #[test]
    fn without_the_os_reader_the_thr_lines_say_so() {
        let w = [WorkerSample::default()];
        let l = thr_lines("run", "client", "", Duration::from_secs(1), None, (&w[..], None), 100.0);
        assert!(l.iter().any(|x| x.starts_with("[THR] os unavailable")), "{l:?}");
        assert!(l.last().unwrap().contains("threads=- cpu_s=- cores=-"), "{l:?}");
    }

    /// `[IOWN]`: window deltas per owner, joined by serial; the lock-wait
    /// gauge is section wall − section thread CPU, over the window's wall;
    /// absent CPU reads `-`.
    #[test]
    fn the_iown_line_carries_deltas_and_the_asleep_gauge() {
        let from = [OwnerSample { serial: 9, polls: 10, q_secs: 1, q_wall_ns: 1_000_000, q_cpu_ns: 400_000, ..Default::default() }];
        let to = [OwnerSample {
            serial: 9,
            path: 1,
            rt: "main".into(),
            polls: 110,
            tx_dgrams: 50,
            q_secs: 11,
            q_wall_ns: 401_000_000,
            q_cpu_ns: 200_400_000,
            drv_on: 7,
            ..Default::default()
        }];
        let l = iown_lines("xfer", "client", " run=1", Duration::from_secs(2), &from, &to);
        assert_eq!(l.len(), 1);
        let s = &l[0];
        assert!(s.starts_with("[IOWN] phase=xfer side=client run=1 path=1 rt=main polls=100 "), "{s}");
        assert!(s.contains(" tx_dg=50 "), "{s}");
        assert!(s.contains(" rx_capped=0 ack_batches=0 ack_dg=0 send_err=0 "), "{s}");
        assert!(s.contains(" q_secs=10 q_wall_us=400000 q_asleep_us=200000 asleep_frac=0.1000 "), "{s}");
        assert!(s.contains(" drv_on=7 drv_off=0 wall_s=2.000"), "{s}");
        // Threading Q3 step 1: the appended deep tokens (zeros when off).
        assert!(s.contains(" qk_snd=0/0/0/0/0/0/0/0 qk_rd=0/0/0/0/0/0/0/0 "), "{s}");
        assert!(s.contains(" fl_sends=0 fl_full=0 fl_full_us=0 "), "{s}");
        // Threading R1: the appended pool / command-depth tokens.
        assert!(s.ends_with(" ibp_hit=0 ibp_miss=0 cmdq_n=0 cmdq_p50=- cmdq_p99=- cmdq_max=-"), "{s}");
        let deep = [OwnerSample {
            serial: 4,
            k_read: crate::task_obs::SpanSample {
                n: 3,
                wall_ns: 9_000,
                cpu_ns: 4_000,
                asl_vol_ns: 5_000,
                vcsw: 1,
                ..Default::default()
            },
            fl_sends: 10,
            fl_full: 2,
            fl_full_ns: 7_000,
            rx_full: 1,
            rx_full_ns: 3_000,
            ..Default::default()
        }];
        let l = iown_lines("xfer", "client", "", Duration::from_secs(1), &[], &deep);
        assert!(l[0].contains(" qk_rd=3/9/4/5/0/0/1/0 "), "{}", l[0]);
        assert!(l[0].contains(" rx_full=1 rx_full_us=3 ack_full=0 ack_full_us=0 fl_sends=10 fl_full=2 fl_full_us=7"), "{}", l[0]);
        let na = [OwnerSample { serial: 3, q_secs: 1, q_cpu_unavailable: true, ..Default::default() }];
        let l = iown_lines("run", "server", "", Duration::from_secs(1), &[], &na);
        assert!(l[0].contains("q_asleep_us=- asleep_frac=-"), "{}", l[0]);
    }

    #[test]
    fn the_lag_line_renders_absent_as_dash() {
        assert_eq!(
            lag_line("run", "client", "", &[], 0),
            "[LAG] phase=run side=client tick_ms=10 n=0 p50_us=- p99_us=- max_us=- min_us=- dropped=0"
        );
        let l = lag_line("xfer", "server", " obj=1", &[100, 200, 300], 0);
        assert!(l.contains("n=3 p50_us=200 p99_us=298 max_us=300 min_us=100 dropped=0"), "{l}");
    }

    #[test]
    fn the_oslag_line_mirrors_the_lag_line_and_counts_skips() {
        assert_eq!(
            os_lag_line("run", "client", "", &[], 0),
            "[OSLAG] phase=run side=client n=0 p50_us=- p99_us=- max_us=- min_us=- skipped=0"
        );
        let l = os_lag_line("xfer", "server", " obj=1", &[100, 200, 300], 2);
        assert_eq!(l, "[OSLAG] phase=xfer side=server obj=1 n=3 p50_us=200 p99_us=298 max_us=300 min_us=100 skipped=2");
    }

    #[test]
    fn the_oslagraw_line_has_the_lagraw_shape() {
        let s = |at_us, late_us| OsLagSample { at_us, late_us, skipped: 0 };
        let v = [s(10_000, 60), s(20_000, 2_500)];
        assert_eq!(
            os_lag_raw_line("client", 1_700_000_000_000_000, Some((5_000, 25_000)), &v, 20_000),
            "[OSLAGRAW] side=client origin_unix_us=1700000000000000 xfer=5000-25000 n=2 printed=2 s=10000:60,20000:2500"
        );
        assert_eq!(os_lag_raw_line("server", 7, None, &[], 20_000), "[OSLAGRAW] side=server origin_unix_us=7 xfer=- n=0 printed=0 s=-");
        assert_eq!(
            os_lag_raw_line("server", 7, None, &v, 1),
            "[OSLAGRAW] side=server origin_unix_us=7 xfer=- n=2 printed=1 s=10000:60"
        );
    }

    #[test]
    fn the_oslag_window_filters_by_time_and_sums_skips() {
        let log = OsLagLog::default();
        log.push(OsLagSample { at_us: 10, late_us: 5, skipped: 1 });
        log.push(OsLagSample { at_us: 20, late_us: 6, skipped: 2 });
        log.push(OsLagSample { at_us: 30, late_us: 7, skipped: 4 });
        assert_eq!(log.window(15, 30), (vec![6, 7], 6));
        assert_eq!(log.window(0, u64::MAX), (vec![5, 6, 7], 7));
        assert_eq!(log.all().len(), 3);
    }

    #[test]
    fn a_missed_deadline_advances_past_now_and_counts_the_skips() {
        // On time: the next tick, nothing skipped.
        assert_eq!(advance_deadline(10, 12, 10), (20, 0));
        // Woke exactly at the next deadline: it is not in the past.
        assert_eq!(advance_deadline(10, 20, 10), (20, 0));
        // 20 and 30 already passed at 35: skipped, next is 40.
        assert_eq!(advance_deadline(10, 35, 10), (40, 2));
        assert_eq!(advance_deadline(10, 31, 10), (40, 2));
        assert_eq!(advance_deadline(10, 30, 10), (30, 1));
    }

    #[test]
    fn schedstat_field_two_is_the_run_delay() {
        assert_eq!(parse_schedstat("123456 789000 42\n"), Some(789_000));
        assert_eq!(parse_schedstat("1"), None);
        assert_eq!(parse_schedstat("1 x 3"), None);
    }

    #[test]
    fn the_thr_os_line_appends_run_delay_only_when_collected() {
        let w = [WorkerSample::default()];
        let t = |ns: Option<Option<u64>>| ThreadSample { tid: 11, comm: "rp-w-0".into(), ticks: 100, delay_ns: ns };
        let line = |from: Option<Vec<ThreadSample>>, to: Vec<ThreadSample>| {
            let l = thr_lines(
                "xfer",
                "client",
                "",
                Duration::from_secs(2),
                from.as_ref().map(|f| (&w[..], Some(&f[..]))),
                (&w[..], Some(&to[..])),
                100.0,
            );
            l.into_iter().find(|x| x.starts_with("[THR] os ")).unwrap()
        };
        // Not collected: the shipped line, byte for byte.
        assert!(line(None, vec![t(None)]).ends_with(" cpu_s=1.000 cores=0.500 wall_s=2.000"));
        // Windowed difference, ns to us.
        let l = line(Some(vec![t(Some(Some(1_000_000)))]), vec![t(Some(Some(4_500_000)))]);
        assert!(l.ends_with(" wall_s=2.000 run_delay_us=3500"), "{l}");
        // A thread born in the window counts from 0.
        assert!(line(Some(vec![]), vec![t(Some(Some(2_000_000)))]).ends_with(" run_delay_us=2000"));
        // Unreadable on either end: `-`.
        assert!(line(None, vec![t(Some(None))]).ends_with(" run_delay_us=-"));
        assert!(line(Some(vec![t(Some(None))]), vec![t(Some(Some(5)))]).ends_with(" run_delay_us=-"));
    }

    /// Linux only (compiled out elsewhere, e.g. on the Windows dev host): the
    /// reference thread produces the 10 ms cadence and stops on request.
    #[cfg(target_os = "linux")]
    #[test]
    fn the_os_reference_thread_samples_at_the_tick() {
        let log = Arc::new(OsLagLog::default());
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let origin_mono = mono_clock_ns();
        let h = spawn_os_ref(log.clone(), origin_mono, stop.clone()).expect("spawn");
        std::thread::sleep(Duration::from_millis(150));
        stop.store(true, Ordering::Relaxed);
        h.join().unwrap();
        let (late, _) = log.window(0, u64::MAX);
        assert!(late.len() >= 10, "{} samples in 150 ms", late.len());
        let all = log.all();
        assert!(all.windows(2).all(|w| w[1].at_us > w[0].at_us), "timeline is monotone");
        assert!(all[0].at_us < 150_000 + 50_000);
    }

    #[test]
    fn the_lagraw_line_carries_origin_xfer_bounds_and_the_samples() {
        let s = |at_us, late_us| LagSample { at_us, late_us };
        let v = [s(10_000, 120), s(20_000, 2_500), s(30_000, 90)];
        assert_eq!(
            lag_raw_line("client", 1_700_000_000_000_000, Some((5_000, 25_000)), &v, 20_000),
            "[LAGRAW] side=client origin_unix_us=1700000000000000 xfer=5000-25000 n=3 printed=3 s=10000:120,20000:2500,30000:90"
        );
        // No xfer window yet, no samples.
        assert_eq!(
            lag_raw_line("server", 7, None, &[], 20_000),
            "[LAGRAW] side=server origin_unix_us=7 xfer=- n=0 printed=0 s=-"
        );
        // The cap trims the printed samples, not n.
        assert_eq!(
            lag_raw_line("server", 7, None, &v, 2),
            "[LAGRAW] side=server origin_unix_us=7 xfer=- n=3 printed=2 s=10000:120,20000:2500"
        );
    }

    /// The runtime names its threads and the probe produces samples whose
    /// p99 ≥ p50 (rule 14 for the instrument: driven alone, deterministic
    /// shape, the branch that fired is visible).
    #[test]
    fn the_runtime_names_workers_and_the_lag_probe_samples() {
        let rt = build_runtime().expect("runtime");
        let name = rt.block_on(async {
            tokio::spawn(async { std::thread::current().name().map(str::to_string) })
                .await
                .unwrap()
        });
        let name = name.expect("worker thread has a name");
        assert!(name.starts_with("rp-w-"), "worker thread name {name}");
        let log = Arc::new(LagLog::default());
        let origin = Instant::now();
        rt.spawn(lag_probe(log.clone(), origin));
        std::thread::sleep(Duration::from_millis(250));
        let lat = log.window(0, u64::MAX);
        assert!(lat.len() >= 10, "lag probe sampled {} ticks in 250 ms", lat.len());
        let p50 = quantile(&lat, 0.5).unwrap();
        let p99 = quantile(&lat, 0.99).unwrap();
        assert!(p99 >= p50, "p99 {p99} < p50 {p50}");
        let m = rt.metrics();
        assert!(m.num_workers() >= 1);
        drop(rt);
    }
}
