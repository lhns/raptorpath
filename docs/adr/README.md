# Architecture Decision Records

This directory contains ADRs for the raptorpath project, ordered by priority.

## Critical (must fix before end-to-end works)

| # | Title | Status |
|---|-------|--------|
| [0002](0002-packet-framing-after-decode.md) | Packet framing after FEC decode | Open |
| [0003](0003-loss-estimation-is-broken.md) | Loss estimation feeds incorrect data | Open |
| [0008](0008-blockstart-not-handled.md) | Receiver doesn't handle BlockStart | Open |
| [0005](0005-ack-mechanism-missing.md) | No ACK/feedback loop | Open |
| [0007](0007-rtt-calculation-broken.md) | RTT depends on clock sync | Open |

## High (traffic stalls, resource leaks, network damage)

| # | Title | Status |
|---|-------|--------|
| [0001](0001-block-assembly-timeout.md) | Block assembly needs flush timeout | Open |
| [0004](0004-decoder-memory-leak.md) | Decoder map grows without bound | Open |
| [0009](0009-no-congestion-control.md) | No congestion control | Open |
| [0011](0011-channel-backpressure.md) | Channels stall under load | Open |

## Medium (UX, performance, operability)

| # | Title | Status |
|---|-------|--------|
| [0006](0006-protocol-hint-block-sizing.md) | Protocol hint should influence block size | Open |
| [0010](0010-handshake-and-versioning.md) | No handshake or protocol versioning | Open |
| [0012](0012-platform-setup-ux.md) | Platform setup too complex | Open |
| [0013](0013-monitoring-and-observability.md) | No runtime monitoring | Open |
| [0014](0014-duplicate-symbol-handling.md) | No duplicate symbol detection | Open |
| [0015](0015-graceful-shutdown.md) | No graceful shutdown | Open |
