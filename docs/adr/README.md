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
