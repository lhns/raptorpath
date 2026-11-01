//! Status §34 — what drives the run-to-run spread of `recv_alloc_bound`
//! (a diagnostic, `#[ignore]`d; it asserts nothing and gates nothing).
//!
//! `recv_alloc_bound` counts process-wide allocations over a 2 MB and a 12 MB
//! in-process loopback on ONE runtime and divides the difference by the
//! extra source symbols. `in_process::run` aborts only the perf server task:
//! the engines it spawned (both ends) keep running on that runtime, so the
//! 2 MB run's engines are still alive — and allocating on their timers —
//! during the 12 MB count. That share scales with the 12 MB run's wall time,
//! not with its symbols.
//!
//! Per pair this prints the same per-symbol figure two ways:
//! * `shared` — exactly the gate's method (one runtime for both counts), plus
//!   each count's wall time and the allocation rate of the leftover engines
//!   in a 1 s idle window after each count;
//! * `fresh` — each count on its own runtime, shut down after the count, so
//!   no engine outlives its count;
//!
//! and the allocation difference per size class (log2 buckets).
//!
//!   cargo test --release -p raptorpath --test recv_alloc_spread_diag -- --ignored --nocapture

#[path = "common/loopback.rs"]
mod loopback;

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

const BUCKETS: usize = 24;
struct Counting;
static ALLOCS: AtomicU64 = AtomicU64::new(0);
static BY_SIZE: [AtomicU64; BUCKETS] = [const { AtomicU64::new(0) }; BUCKETS];

fn note(size: usize) {
    ALLOCS.fetch_add(1, Ordering::Relaxed);
    let b = (usize::BITS - size.leading_zeros()) as usize;
    BY_SIZE[b.min(BUCKETS - 1)].fetch_add(1, Ordering::Relaxed);
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        note(l.size());
        unsafe { System.alloc(l) }
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        note(l.size());
        unsafe { System.alloc_zeroed(l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        note(n);
        unsafe { System.realloc(p, l, n) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

const SMALL: usize = 2_000_000;
const LARGE: usize = 12_000_000;
const PAYLOAD: f64 = 1198.0;

fn snap() -> (u64, [u64; BUCKETS]) {
    let mut b = [0u64; BUCKETS];
    for (i, x) in BY_SIZE.iter().enumerate() {
        b[i] = x.load(Ordering::Relaxed);
    }
    (ALLOCS.load(Ordering::Relaxed), b)
}

struct Count {
    allocs: u64,
    buckets: [u64; BUCKETS],
    wall_s: f64,
}

async fn count(bytes: usize) -> Count {
    use loopback::in_process::{cfgs, ports, resolve, run};
    let (s, c) = cfgs(&ports(1), "bulk", true);
    let (srv, cli) = (resolve(&s), resolve(&c));
    let (a0, b0) = snap();
    let t0 = Instant::now();
    run(srv, cli, bytes, 1, Duration::from_secs(120), "recv-alloc-spread loopback").await;
    let wall_s = t0.elapsed().as_secs_f64();
    let (a1, b1) = snap();
    let mut buckets = [0u64; BUCKETS];
    for i in 0..BUCKETS {
        buckets[i] = b1[i] - b0[i];
    }
    Count { allocs: a1 - a0, buckets, wall_s }
}

/// Allocations per second while nothing is being transferred (the engines
/// left running by earlier counts, if any).
async fn idle_rate() -> f64 {
    let a0 = ALLOCS.load(Ordering::Relaxed);
    tokio::time::sleep(Duration::from_secs(1)).await;
    (ALLOCS.load(Ordering::Relaxed) - a0) as f64
}

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread().worker_threads(4).enable_all().build().unwrap()
}

fn per_sym(s: &Count, l: &Count) -> f64 {
    (l.allocs as f64 - s.allocs as f64) / ((LARGE - SMALL) as f64 / PAYLOAD)
}

fn bucket_line(s: &Count, l: &Count) -> String {
    let syms = (LARGE - SMALL) as f64 / PAYLOAD;
    (0..BUCKETS)
        .filter(|&i| s.buckets[i] + l.buckets[i] > 0)
        .map(|i| {
            let d = (l.buckets[i] as f64 - s.buckets[i] as f64) / syms;
            // bucket i holds sizes in [2^(i-1), 2^i)
            format!("b{}={:.3}", if i == 0 { 0 } else { 1u64 << (i - 1) }, d)
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
#[ignore = "status §34 diagnostic; run explicitly with --ignored"]
fn recv_alloc_spread_diagnostic() {
    let pairs: usize = std::env::var("RAS_PAIRS").ok().and_then(|v| v.parse().ok()).unwrap_or(6);
    for pair in 1..=pairs {
        // shared: the gate's method.
        let r = rt();
        let (s, idle_s, l, idle_l) = r.block_on(async {
            let s = count(SMALL).await;
            let idle_s = idle_rate().await;
            let l = count(LARGE).await;
            let idle_l = idle_rate().await;
            (s, idle_s, l, idle_l)
        });
        r.shutdown_timeout(Duration::from_secs(5));
        println!(
            "RAS variant=shared pair={pair} small={} large={} per_sym={:.3} t_small={:.3} t_large={:.3} idle_after_small={:.0} idle_after_large={:.0} {}",
            s.allocs, l.allocs, per_sym(&s, &l), s.wall_s, l.wall_s, idle_s, idle_l, bucket_line(&s, &l)
        );

        // fresh: one runtime per count; nothing outlives its count.
        let r = rt();
        let s = r.block_on(count(SMALL));
        r.shutdown_timeout(Duration::from_secs(5));
        let r = rt();
        let (l, idle_l) = r.block_on(async {
            let l = count(LARGE).await;
            (l, idle_rate().await)
        });
        r.shutdown_timeout(Duration::from_secs(5));
        println!(
            "RAS variant=fresh pair={pair} small={} large={} per_sym={:.3} t_small={:.3} t_large={:.3} idle_after_large={:.0} {}",
            s.allocs, l.allocs, per_sym(&s, &l), s.wall_s, l.wall_s, idle_l, bucket_line(&s, &l)
        );
    }
}
