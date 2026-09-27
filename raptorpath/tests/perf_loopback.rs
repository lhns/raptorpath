//! In-process loopback test for the rp-native perf mode: a perf server
//! and perf client, each running the real engine over a memory TUN,
//! exchange a small object over 127.0.0.1 (real QUIC, no kernel TUN,
//! no routes/DNS). Guards the run_with_tun seam and the perf object
//! protocol end to end.

#[path = "common/loopback.rs"]
mod loopback;

use std::time::Duration;

use loopback::in_process::{cfgs, ports, resolve, run};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn perf_loopback_small_object() {
    // The client bails if the warm-up object is never acked and only
    // returns Ok after every run completed or timed out; bounding the
    // whole thing well under the 300 s run timeout means Ok == the
    // object round-tripped (chunks delivered, reassembled, acked).
    loopback::in_process_loopback("bulk", false, 200_000, 2, "perf loopback").await;
}

/// The same loopback exchange over the reliable sliding-window pipeline
/// (`window_reliable`, bulk hint → windowed RLC). Guards the retention path
/// end to end: the sent-data store fills and drains on real peer WindowAcks
/// (removal by ack only), the receiver's reliable reorder buffer delivers in
/// order, and the transfer completes, i.e. the retention plumbing never
/// wedges a clean link.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn perf_loopback_reliable_window() {
    loopback::in_process_loopback("bulk", true, 200_000, 2, "reliable-window perf loopback").await;
}

/// Out-of-order object delivery (`window_out_of_order`, the H → ∞ corner of
/// paper §4.11) over the reliable-window loopback. The receiver hands each
/// decoded symbol to the consumer the instant it decodes (bypassing the
/// in-order frontier) and the sender's retention backpressure is relaxed;
/// the perf server reassembles by offset and acks on total-decoded. It must
/// complete without wedging (the object protocol only acks when
/// st.got.len() == total, so completion is the all-bytes-present check).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn perf_loopback_out_of_order_object() {
    let (mut s, mut c) = cfgs(&ports(1), "bulk", true);
    s.window_out_of_order = Some(true);
    c.window_out_of_order = Some(true);
    let (srv, cli) = (resolve(&s), resolve(&c));
    assert!(srv.window_reliable && srv.window_out_of_order);

    // A larger object (spans many windows, so out-of-order delivery is
    // actually exercised across window boundaries) still completes.
    run(srv, cli, 1_000_000, 2, Duration::from_secs(60), "out-of-order perf loopback").await;
}

/// Coded-object mode (paper §5.5): the reliable-window loopback with
/// `window_coded_only`. The sender emits only coded (random-linear-
/// combination) symbols over the window, no raw systematic source on the wire
/// during normal flow, and the receiver reconstructs every source seq by
/// Gaussian elimination and delivers it out of order (reassembly by offset).
/// With no systematic passthrough the object must still complete with all
/// bytes (the perf server only acks when st.got.len() == total, so completion
/// is the all-bytes-present, decode-on-K check).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn perf_loopback_coded_object() {
    let (mut s, mut c) = cfgs(&ports(1), "bulk", true);
    s.window_coded_only = Some(true);
    c.window_coded_only = Some(true);
    let (srv, cli) = (resolve(&s), resolve(&c));
    assert!(srv.window_reliable && srv.window_coded_only);

    // A multi-window object: coded-only must reconstruct every seq purely by
    // GE (no systematic passthrough) across window boundaries and complete.
    run(srv, cli, 1_000_000, 2, Duration::from_secs(60), "coded-object perf loopback").await;
}

/// Generation-based cross-path fungible coding (paper §5.5). The sender
/// partitions the object into fixed generations and emits only coded
/// (random-linear-combination) symbols within each generation, a stable
/// coding anchor. Any K_G independent coded symbols from any path reconstruct
/// a generation, which decodes out of order the instant K_G arrive; per-seq
/// ARQ is off beneath the code (the receiver installs no NACK producer in
/// generation mode). With no systematic passthrough and no per-seq
/// retransmit, the object must still complete with every byte purely by
/// per-generation Gaussian elimination (the perf server acks only when
/// st.got.len() == total, so completion is the all-bytes-present, decode-on-K
/// check). A 1 MB object at the default G=384 spans ~3 generations decoded
/// out of order.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn perf_loopback_generation_object() {
    // Uses the production defaults (RWM_GEN=384, RWM_PIPELINE=2); a 1 MB object
    // spans ~3 generations. (Small-generation stress — many generations,
    // out-of-order, with loss — is covered by the codec unit test
    // `generation::tests::generations_decode_on_k_out_of_order_with_loss`. Env
    // knobs are not set here: cargo runs tests in parallel and RWM_GEN is
    // process-global, so two tests writing it would race.)
    let (mut s, mut c) = cfgs(&ports(1), "bulk", true);
    s.window_generation_coding = Some(true);
    c.window_generation_coding = Some(true);
    let (srv, cli) = (resolve(&s), resolve(&c));
    assert!(srv.window_reliable && srv.window_generation_coding);

    run(srv, cli, 1_000_000, 2, Duration::from_secs(60), "generation-coded perf loopback").await;
}

/// Generation coding over a dual path (two loopback links). Coded symbols are
/// striped ∝ goodput across both paths by the placement law (paper §5.7), and
/// a generation completes on the pooled K_G arrivals from either path. Guards
/// that the object completes with all bytes when coded symbols for one
/// generation are split across two independent paths.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn perf_loopback_generation_dual_path() {
    let (mut s, mut c) = cfgs(&ports(2), "bulk", true);
    s.window_generation_coding = Some(true);
    c.window_generation_coding = Some(true);
    let (srv, cli) = (resolve(&s), resolve(&c));

    run(srv, cli, 1_000_000, 2, Duration::from_secs(60), "dual-path generation perf loopback").await;
}

/// Multi-generation (≥3 generations) completion over a dual path, driven by
/// the per-generation deficit-feedback loop (paper §5.5). A 2 MB object at
/// the default G=384 (~1.4 kB symbols) spans ≥3 generations, so the
/// sealed-generation frontier must advance repeatedly: generation g completes
/// on the pooled K_g coded arrivals from both paths, the receiver reports
/// each frontier generation's residual deficit, and the sender emits exactly
/// that residual for the stalled generation. Per-seq ARQ is off (generation
/// mode installs no NACK producer), so completion comes purely from
/// generation-level recovery; this proves the deficit loop pipelines many
/// generations to completion, not just the first. The loopback is lossless.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn perf_loopback_generation_multi_dual_path() {
    let (mut s, mut c) = cfgs(&ports(2), "bulk", true);
    s.window_generation_coding = Some(true);
    c.window_generation_coding = Some(true);
    let (srv, cli) = (resolve(&s), resolve(&c));
    assert!(srv.window_reliable && srv.window_generation_coding);

    // 2 MB → ≥3 generations at the default G=384. Completion (the perf server
    // acks only when every byte is present) is the ≥3-generation,
    // frontier-advancing, deficit-loop proof.
    run(srv, cli, 2_000_000, 1, Duration::from_secs(90), "multi-generation dual-path perf loopback").await;
}

/// Systematic + deficit-repair (paper §5.5) over a dual path. The raw
/// systematic source rides the wire as primary (striped ∝ goodput across
/// both loopback links, delivered out of order with zero decode); coded
/// symbols are windowed repair only (ceil(len·r) proactive per generation
/// plus a deficit-driven top-up), with per-seq ARQ off (generation-mode
/// receive path). A multi-MB object spans several generations, so this guards
/// source pass-through + windowed cross-path repair + the deficit-feedback
/// frontier advance, composed with the perf object protocol. The loopback is
/// lossless: it proves the plumbing never wedges and every byte round-trips.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn perf_loopback_systematic_repair_dual_path() {
    let (mut s, mut c) = cfgs(&ports(2), "bulk", true);
    s.window_systematic_repair = Some(true);
    c.window_systematic_repair = Some(true);
    let (srv, cli) = (resolve(&s), resolve(&c));
    assert!(srv.window_reliable && srv.window_systematic_repair);

    // 2 MB → several generations at the default G. Completion (the perf server
    // acks only when every byte is present) is the end-to-end proof.
    run(srv, cli, 2_000_000, 1, Duration::from_secs(90), "systematic-repair dual-path perf loopback").await;
}
