# ADR-0013: No Runtime Monitoring or Observability

## Status
**Open** — usability and debugging issue

## Context
A multipath FEC system has many moving parts. When something goes wrong (high latency, stalls, path failures), the operator needs visibility into what's happening.

## Problem
Currently:
- Only `tracing` log output at INFO level
- No per-path statistics visible to the user
- `FecDiagnostics` struct exists but is never exposed
- No way to query runtime state without attaching a debugger
- No way to tell if a path is degraded vs dead vs congested
- No metrics on FEC overhead (how many repair symbols actually needed vs sent)

## Decision Required
Implement a multi-layer observability stack:

### Layer 1: Structured logging (immediate)
Already using `tracing`. Add structured spans per-path and per-block:
```rust
let span = tracing::info_span!("path", id = path_id, rtt_ms = %rtt.as_millis());
```

### Layer 2: Status endpoint (short-term)
Expose a local HTTP or Unix socket endpoint with JSON stats:
```
GET /status → {
  "paths": [
    { "id": 0, "loss": 0.02, "rtt_ms": 15, "throughput_mbps": 50, "state": "active" },
    { "id": 1, "loss": 0.10, "rtt_ms": 45, "throughput_mbps": 10, "state": "degraded" }
  ],
  "fec": {
    "target_tail_loss": 1e-5,
    "actual_failure_rate": 2e-6,
    "overhead_ratio": 0.08,
    "pi_correction": 1.2
  },
  "blocks": {
    "encoded": 15230,
    "decoded_ok": 15228,
    "decoded_fail": 2,
    "pending": 3
  }
}
```

### Layer 3: CLI companion (medium-term)
`raptorpath status` command that connects to running instance:
```
$ raptorpath status
Path 0 (WiFi)   ████████████░░░  RTT: 12ms  Loss: 1.2%  48 Mbps
Path 1 (LTE)    ██████░░░░░░░░░  RTT: 45ms  Loss: 8.1%  12 Mbps
FEC overhead: 9.3%  Tail loss: <1e-6  Blocks: 15230 ok / 2 fail
```

### Layer 4: Prometheus metrics (long-term)
Export metrics in Prometheus format for grafana dashboards.

## Consequences
- Operators can debug issues without code changes
- Performance tuning becomes data-driven
- Small runtime overhead for metrics collection
