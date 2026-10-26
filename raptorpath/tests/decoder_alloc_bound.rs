//! Threading R3 (status §22): heap allocations per in-order source symbol in
//! `UnifiedDecoder` alone, bounded. Before R3 an in-order source cost ~5-6
//! allocations (zeroed pad buffer, worklist Vec + VecDeque, payload clones,
//! the output Bytes, a BTreeSet node, a HashSet slot). After: the output
//! `Vec` plus the amortised BTreeMap node (the payload is the received
//! `Bytes`, the worklist is reused, there is no `delivered` / `seen`).
//!
//! Sources are generated before the counted region; only `add_symbol` and
//! `advance` (lag 400, every 1 000 sources) are counted.
//!
//!   cargo test --release -p raptorpath --test decoder_alloc_bound -- --nocapture

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, Ordering};

use raptorpath::fec::{RlcWindowEncoder, UnifiedDecoder, WindowDecoder, WindowEncoder};

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

/// Per source: 1 for the output Vec plus the BTreeMap node amortised over
/// its entries (~0.1-0.2). Pre-R3 by reading the code: 5-6.
const BOUND: f64 = 1.5;

#[test]
fn in_order_sources_cost_at_most_one_and_a_half_allocations_each() {
    let ss = 1182u16;
    let n = 20_000u64;
    let mut enc = RlcWindowEncoder::new(ss);
    let payload = vec![0x5au8; ss as usize];
    let syms: Vec<_> = (0..n).map(|_| enc.add_source(&payload)).collect();
    let mut dec = UnifiedDecoder::new(ss);

    let a0 = ALLOCS.load(Ordering::Relaxed);
    let mut delivered = 0u64;
    for (i, s) in syms.iter().enumerate() {
        delivered += dec.add_symbol(s).len() as u64;
        let i = i as u64;
        if i % 1_000 == 999 {
            dec.advance(i.saturating_sub(400));
        }
    }
    let allocs = ALLOCS.load(Ordering::Relaxed) - a0;
    assert_eq!(delivered, n, "every source delivered exactly once");
    let per = allocs as f64 / n as f64;
    println!("DEC_ALLOC allocs={allocs} sources={n} allocs/source={per:.3} bound={BOUND}");
    assert!(per <= BOUND, "allocations per source {per:.3} exceed the bound {BOUND}");
}
