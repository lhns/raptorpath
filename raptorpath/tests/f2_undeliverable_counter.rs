//! The undeliverable-symbol counter (`net::undeliverable_symbols`, printed as
//! `undeliv=` on the receiver's `[CTLD]` line under `RWM_DIAG`) stays 0 on a
//! healthy Bulk and a healthy Realtime loopback. Its increment in the F2
//! repro case is `net::tests::f2_packed_symbol_read_unpacked_is_counted`.
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
}
