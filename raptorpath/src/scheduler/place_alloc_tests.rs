//! Status §41 host gate: `place_costs` with `RWM_PLACE_WIRE` armed allocates
//! nothing beyond what the arm-off call allocates (its one result `Vec`), and
//! the armed-vs-off cost is measured in ns (reported, `#[ignore]`).
//!
//! The counter is thread-local so concurrent unit tests cannot add noise.

use super::*;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

struct Counting;
thread_local! {
    static ALLOCS: Cell<u64> = const { Cell::new(0) };
}
fn bump() {
    // `try_with`: allocation during thread teardown must not panic.
    let _ = ALLOCS.try_with(|c| c.set(c.get() + 1));
}
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        bump();
        unsafe { System.alloc(l) }
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        bump();
        unsafe { System.alloc_zeroed(l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        bump();
        unsafe { System.realloc(p, l, n) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
}
#[global_allocator]
static GLOBAL: Counting = Counting;

fn allocs() -> u64 {
    ALLOCS.with(|c| c.get())
}

fn two_leg(armed: bool) -> Scheduler {
    let mut sched = Scheduler::new_with_hint(Arc::new(MockClock::new()), ProtocolHint::Auto);
    sched.set_place_wire(armed);
    for (id, s_ms, wire_ms) in [(0, 20.0, 18.0), (1, 60.0, 55.0)] {
        sched.add_path(id);
        let p = sched.path_mut(id).unwrap();
        for _ in 0..80 {
            p.record_rtt_sample(Duration::from_secs_f64(s_ms * 1e-3));
        }
        p.set_wire_rtt(Duration::from_secs_f64(wire_ms * 1e-3));
        p.cwnd = 100;
    }
    sched
}

/// Heap blocks allocated by one `place_costs` call (after a warm-up call).
fn allocs_of_one_call(sched: &Scheduler) -> u64 {
    let _ = sched.place_costs(false, &[]);
    let a0 = allocs();
    let out = sched.place_costs(false, &[]);
    let n = allocs() - a0;
    assert_eq!(out.len(), 2);
    n
}

#[test]
fn armed_place_costs_allocates_exactly_what_off_does() {
    let off = allocs_of_one_call(&two_leg(false));
    let on = allocs_of_one_call(&two_leg(true));
    // The result `Vec` is the only block main's `place_costs` allocates.
    assert_eq!(off, 1, "arm off: exactly the result Vec (main's work)");
    assert_eq!(on - off, 0, "armed minus off must be exactly 0 heap blocks");
}

/// Median ns per `place_costs` call, armed vs off, on the same two-leg state.
///   cargo test --release -p raptorpath --lib place_costs_ns -- --ignored --nocapture
#[test]
#[ignore = "host micro-measurement: run explicitly, release build"]
fn place_costs_ns_armed_vs_off() {
    fn median_ns(sched: &Scheduler) -> f64 {
        // Windows' `Instant` ticks at 100 ns, so time batches and divide:
        // 2000 batches x 100 calls = 2*10^5 calls, median batch.
        const BATCHES: usize = 2000;
        const PER: usize = 100;
        let mut v = Vec::with_capacity(BATCHES);
        for _ in 0..BATCHES {
            let t = std::time::Instant::now();
            for _ in 0..PER {
                std::hint::black_box(sched.place_costs(false, &[]));
            }
            v.push(t.elapsed().as_nanos() as f64 / PER as f64);
        }
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v[BATCHES / 2]
    }
    let off = median_ns(&two_leg(false));
    let on = median_ns(&two_leg(true));
    println!("PLACE_COSTS_NS off={off:.0} armed={on:.0} delta={:.0}", on - off);
}
