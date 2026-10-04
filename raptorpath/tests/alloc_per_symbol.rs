//! Microbench (`#[ignore]`): heap allocations per transferred source symbol
//! on the reliable-window data path, process-wide (sender + receiver + quinn
//! in one process), with a counting global allocator.
//!
//! Method: run the in-process perf loopback (real engine, memory TUN, real
//! QUIC on 127.0.0.1, bulk hint, reliable window) for a small and a large
//! object and difference the two allocation counts, so the fixed setup cost
//! (handshake, certs, task spawn) cancels; divide by the difference in
//! source symbols (object bytes / symbol payload). Uses only public API, so
//! the same file builds on a tree before and after a hot-path change.
//!
//!   cargo test --release -p raptorpath --test alloc_per_symbol -- --ignored --nocapture

#[path = "common/loopback.rs"]
mod loopback;

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

struct Counting;
static ALLOCS: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(l.size() as u64, Ordering::Relaxed);
        unsafe { System.alloc(l) }
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(l.size() as u64, Ordering::Relaxed);
        unsafe { System.alloc_zeroed(l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(n as u64, Ordering::Relaxed);
        unsafe { System.realloc(p, l, n) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

async fn count(bytes: usize) -> (u64, u64) {
    use loopback::in_process::{cfgs, ports, resolve, run};
    let (s, c) = cfgs(&ports(1), "bulk", true);
    let (srv, cli) = (resolve(&s), resolve(&c));
    let a0 = ALLOCS.load(Ordering::Relaxed);
    let b0 = BYTES.load(Ordering::Relaxed);
    run(srv, cli, bytes, 1, Duration::from_secs(120), "alloc-per-symbol loopback").await;
    (ALLOCS.load(Ordering::Relaxed) - a0, BYTES.load(Ordering::Relaxed) - b0)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "microbench: run explicitly with --ignored --nocapture"]
async fn allocations_per_source_symbol() {
    // Symbol payload of the default window framing (symbol_size − 2-byte
    // length prefix); the perf object rides full-size packets.
    let payload = std::env::var("ALLOC_BENCH_PAYLOAD")
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(1198.0);
    let (small, large) = (2_000_000usize, 20_000_000usize);
    let (a_s, b_s) = count(small).await;
    let (a_l, b_l) = count(large).await;
    let syms = (large - small) as f64 / payload;
    let per_sym = (a_l as f64 - a_s as f64) / syms;
    let bytes_per_sym = (b_l as f64 - b_s as f64) / syms;
    println!(
        "ALLOC_BENCH small={small}B allocs={a_s} | large={large}B allocs={a_l} | \
         allocs/symbol={per_sym:.2} alloc_bytes/symbol={bytes_per_sym:.0} (payload={payload})"
    );
}
