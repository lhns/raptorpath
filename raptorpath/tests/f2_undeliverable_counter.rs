//! The undeliverable-symbol counter (`net::undeliverable_symbols`, printed as
//! `undeliv=` on the receiver's `[CTLD]` line under `RWM_DIAG`) stays 0 on a
//! healthy Bulk and a healthy Realtime loopback. Its increment on an
//! unframable symbol is `net::tests::f2_unframable_symbol_is_counted_once`.
//!
//! Own test binary: the counter is process-global.

#[path = "common/loopback.rs"]
mod loopback;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn counter_is_zero_on_bulk_and_realtime_loopback() {
    loopback::in_process_loopback("bulk", false, 200_000, 1, "f2 counter bulk").await;
    assert_eq!(raptorpath::net::undeliverable_symbols(), 0, "bulk loopback");
    loopback::in_process_loopback("realtime", false, 20_000, 1, "f2 counter realtime").await;
    assert_eq!(raptorpath::net::undeliverable_symbols(), 0, "realtime loopback");
    // Same-size pairs: every decoder takes its own size back from the peer.
    let sizes = raptorpath::net::RECV_SYMBOL_SIZES.lock().unwrap().clone();
    assert!(sizes.iter().all(|&(local, decoder)| local == decoder), "{sizes:?}");
    for s in [1200u16, 512] {
        assert!(sizes.contains(&(s, s)), "no ({s}, {s}) entry in {sizes:?}");
    }
}
