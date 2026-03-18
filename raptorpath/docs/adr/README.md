# Architecture Decision Records

This directory contains ADRs for the raptorpath project, ordered by priority.

## Critical (must fix before end-to-end works)

| # | Title | Status |
|---|-------|--------|
| [0002](0002-packet-framing-after-decode.md) | Packet framing after FEC decode | Resolved |
| [0003](0003-loss-estimation-is-broken.md) | Loss estimation feeds incorrect data | Resolved |
| [0008](0008-blockstart-not-handled.md) | Receiver doesn't handle BlockStart | Resolved |
| [0005](0005-ack-mechanism-missing.md) | No ACK/feedback loop | Resolved |
| [0007](0007-rtt-calculation-broken.md) | RTT depends on clock sync | Resolved |

## High (traffic stalls, resource leaks, network damage)

| # | Title | Status |
|---|-------|--------|
| [0001](0001-block-assembly-timeout.md) | Block assembly needs flush timeout | Resolved |
| [0004](0004-decoder-memory-leak.md) | Decoder map grows without bound | Resolved |
| [0009](0009-no-congestion-control.md) | No congestion control | Resolved |
| [0011](0011-channel-backpressure.md) | Channels stall under load | Resolved |

## Medium (UX, performance, operability)

| # | Title | Status |
|---|-------|--------|
| [0006](0006-protocol-hint-block-sizing.md) | Protocol hint should influence block size | Resolved |
| [0010](0010-handshake-and-versioning.md) | No handshake or protocol versioning | Resolved |
| [0012](0012-platform-setup-ux.md) | Platform setup too complex | Resolved |
| [0013](0013-monitoring-and-observability.md) | No runtime monitoring | Resolved |
| [0014](0014-duplicate-symbol-handling.md) | No duplicate symbol detection | Resolved |
| [0015](0015-graceful-shutdown.md) | No graceful shutdown | Resolved |

## Features

| # | Title | Status |
|---|-------|--------|
| [0016](0016-block-interleaving.md) | Block interleaving for burst loss resilience | Resolved |
| [0017](0017-mtu-aware-symbol-sizing.md) | MTU-aware symbol sizing via PMTU discovery | Resolved |
| [0018](0018-connection-migration.md) | Runtime connection migration via HTTP API | Resolved |
| [0019](0019-bbr-delay-based-cc.md) | BBR-style delay-based congestion control | Resolved |
| [0020](0020-tls-cert-pinning.md) | Optional TLS certificate pinning | Resolved |
| [0021](0021-swappable-fec-backend.md) | Swappable FEC backend (RaptorQ + METTLE) | Resolved |
| [0022](0022-sliding-window-fec.md) | Sliding window FEC architecture (RS + RLC + window pipeline) | Resolved |
| [0023](0023-gilbert-elliott-loss-model.md) | Gilbert-Elliott HMM for bursty loss estimation | Resolved |
| [0024](0024-bbr-probe-rtt-phase.md) | BBR ProbeRTT phase for min_rtt freshness | Resolved |
| [0025](0025-window-nack-sender-repair.md) | WindowNack sender-side targeted repair | Resolved |
| [0026](0026-multipath-window-scheduling.md) | Multipath window scheduling (RTT/goodput + redundant) | Resolved |
| [0027](0027-streaming-codes.md) | Streaming codes (Badr/Martinian delay-optimal) | Resolved |
| [0028](0028-mettle-performance-analysis.md) | METTLE performance analysis — edge probability bug | Resolved |
| [0029](0029-tapered-repair-interleaving.md) | Tapered repair interleaving (exponential decay + window burst) | Resolved |
| [0030](0030-runtime-backend-switching.md) | Runtime FEC backend switching (loss-based heuristic + flush protocol) | Resolved |
| [0031](0031-network-simulation-harness.md) | Network simulation harness (SimChannel + component tests) | Resolved |
| [0032](0032-benchmark-recommendations.md) | Ablation benchmark recommendations (PI window fix, GE default, trial count) | Resolved |
| [0033](0033-pipeline-ablation-benchmark.md) | Full-pipeline ablation benchmark (ProbeRTT, reorder, NACK, auto-switch, multipath) | Resolved |
| [0034](0034-tradeoff-ablation-benchmark.md) | Per-feature tradeoff ablation (latency, ordering, burst recovery, efficiency) | Resolved |
| [0035](0035-algorithm-recommendations.md) | Algorithm recommendations and metric architecture review | Resolved |
| [0036](0036-transport-comparison-benchmark.md) | Raptorpath vs reliable QUIC/MPTCP transport comparison benchmark | Accepted |
| [0037](0037-nack-source-retransmit.md) | NACK source retransmission, cross-path repair, fractional repair accumulator | Accepted |
| [0038](0038-benchmark-overhead-taxonomy.md) | Benchmark overhead taxonomy and methodology documentation | Accepted |
| [0039](0039-overhead-reduction.md) | Overhead reduction: benchmark repair floor fix and window-mode symbol packing | Accepted |
| [0040](0040-benchmark-repair-alignment.md) | Benchmark repair alignment + multi-backend (RLC/METTLE/RaptorQ) comparison | Accepted |
| [0041](0041-simd-gf256.md) | SIMD-accelerated GF(2^8) multiply-accumulate (split-table PSHUFB) | Accepted |
