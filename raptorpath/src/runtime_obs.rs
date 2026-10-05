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
//! completion; [`snapshot`] at the start, [`emit`] at the end), so per-thread
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
        "[LAG] phase={phase} side={side}{extra} tick_ms={} n={} p50_us={} p99_us={} max_us={} dropped={dropped}",
        LAG_TICK.as_millis(),
        late_us.len(),
        f(0.5),
        f(0.99),
        late_us.iter().max().map_or_else(|| "-".to_string(), |m| m.to_string()),
    )
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
}

/// A point-in-time reading of every counter `[THR]` prints.
#[derive(Clone, Debug)]
pub struct Snapshot {
    at: Instant,
    workers: Vec<WorkerSample>,
    /// `None` where the OS reader is unavailable (non-Linux).
    threads: Option<Vec<ThreadSample>>,
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
fn read_threads() -> Option<Vec<ThreadSample>> {
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
            out.push(ThreadSample { tid, comm, ticks });
        }
    }
    out.sort_by_key(|t| t.tid);
    Some(out)
}

#[cfg(not(target_os = "linux"))]
fn read_threads() -> Option<Vec<ThreadSample>> {
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
                out.push(format!(
                    "[THR] os phase={phase} side={side}{extra} tid={} comm={} cpu_s={cpu:.3} cores={:.3} wall_s={wall_s:.3}",
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
    lag: Arc<LagLog>,
    side: OnceLock<&'static str>,
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

/// Whether the observer is armed in this process.
pub fn armed() -> bool {
    OBS.get().is_some()
}

/// The armed path of [`arm`].
fn arm_unconditionally() {
    let handle = tokio::runtime::Handle::current();
    let origin = Instant::now();
    let lag = Arc::new(LagLog::default());
    if OBS
        .set(Obs { handle: handle.clone(), origin, lag: lag.clone(), side: OnceLock::new() })
        .is_ok()
    {
        handle.spawn(lag_probe(lag, origin));
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
    Some(Snapshot { at: Instant::now(), workers, threads: read_threads() })
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
    let lat = o.lag.window(a, b);
    crate::readout!("{}", lag_line("xfer", side, extra, &lat, o.lag.dropped.load(Ordering::Relaxed)));
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
        let from_t = [ThreadSample { tid: 11, comm: "rp-w-0".into(), ticks: 50 }];
        let to_t = [
            ThreadSample { tid: 11, comm: "rp-w-0".into(), ticks: 150 },
            // A thread born inside the window counts from 0.
            ThreadSample { tid: 12, comm: "raptorpath".into(), ticks: 40 },
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

    #[test]
    fn the_lag_line_renders_absent_as_dash() {
        assert_eq!(
            lag_line("run", "client", "", &[], 0),
            "[LAG] phase=run side=client tick_ms=10 n=0 p50_us=- p99_us=- max_us=- dropped=0"
        );
        let l = lag_line("xfer", "server", " obj=1", &[100, 200, 300], 0);
        assert!(l.contains("n=3 p50_us=200 p99_us=298 max_us=300"), "{l}");
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
