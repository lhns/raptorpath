//! Threading R1 — heap allocations per processed in-order source symbol,
//! bounded (a regression gate for the receiver's per-datagram cost).
//!
//! Method: the in-process perf loopback (real engine both ends, memory TUN,
//! real QUIC on 127.0.0.1, bulk hint, reliable window) for a small and a
//! large object, with a counting global allocator; the two counts are
//! differenced so the fixed setup (handshake, certs, task spawn) cancels, and
//! divided by the extra source symbols. The count is PROCESS-WIDE (sender +
//! receiver + quinn): the sender and quinn are untouched by R1, so the delta
//! between trees is the receiver path (the inbound `Vec` per owner poll, the
//! per-message `lat_arrivals` and `received_ids` `Vec`s, the received-seq
//! tree node per symbol, the `[LAT]` hash map). Uses only public API, so the
//! same file builds on the tree before and after the change.
//!
//! The bound is set from the fixed code (see `BOUND`): measured on the VM,
//! release, 1 path, loss-free, ce60079 vs R1. Red on ce60079 (25.1-25.3), green on
//! R1 (22.7-22.9).
//!
//!   cargo test --release -p raptorpath --test recv_alloc_bound -- --nocapture

#[path = "common/loopback.rs"]
mod loopback;

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

struct Counting;
static ALLOCS: AtomicU64 = AtomicU64::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc(l) }
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc_zeroed(l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.realloc(p, l, n) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

/// Allocations per source symbol the whole loopback may spend. Set from the
/// fixed code, release, 1 path, VM (2 MB vs 12 MB objects), repeated runs:
/// R1 22.74-22.94 (9 runs), ce60079 25.05-25.34 (10 runs). 23.5 is R1's worst
/// run + 0.56 (about eight times its spread); the whole change is worth 2.4,
/// so undoing it fails by 1.6. Undoing one fix alone (0.4 to 1) may not be
/// caught: this is a regression gate for the set, not for each member.
const BOUND: f64 = 23.5;

async fn count(bytes: usize) -> u64 {
    use loopback::in_process::{cfgs, ports, resolve, run};
    let (s, c) = cfgs(&ports(1), "bulk", true);
    let (srv, cli) = (resolve(&s), resolve(&c));
    let a0 = ALLOCS.load(Ordering::Relaxed);
    run(srv, cli, bytes, 1, Duration::from_secs(120), "recv-alloc-bound loopback").await;
    ALLOCS.load(Ordering::Relaxed) - a0
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn allocations_per_source_symbol_stay_under_the_bound() {
    // Symbol payload of the default window framing (symbol_size - 2-byte
    // length prefix); the perf object rides full-size packets.
    let payload = 1198.0;
    let (small, large) = (2_000_000usize, 12_000_000usize);
    let a_s = count(small).await;
    let a_l = count(large).await;
    let per_sym = (a_l as f64 - a_s as f64) / ((large - small) as f64 / payload);
    println!("RECV_ALLOC small={a_s} large={a_l} allocs/symbol={per_sym:.3} bound={BOUND}");
    assert!(
        per_sym <= BOUND,
        "allocations per source symbol {per_sym:.3} exceed the bound {BOUND}"
    );
}
