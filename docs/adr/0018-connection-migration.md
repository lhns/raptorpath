# ADR-0018: Runtime Connection Migration

## Status: Resolved

## Context

Raptorpath bonds multiple network paths, but all paths must be configured at
startup. In practice, network interfaces come and go (e.g., connecting to WiFi,
switching cellular networks, plugging in Ethernet). Users need the ability to
add or remove paths without restarting the tunnel.

## Decision

Implement runtime path management through three layers:

### 1. HTTP API for path management

Extend the existing monitoring HTTP server with two new endpoints:

- **`POST /paths`** — add a path at runtime. Body: `{ bind_addr, peer_addr? }`.
  Server-side peers omit `peer_addr` (they accept incoming connections).
- **`DELETE /paths/{id}`** — remove a path, closing its QUIC connection and
  cleaning up the endpoint.

### 2. Lock-free transport with DashMap

Replace `HashMap` with `DashMap` in `QuicTransport` for `endpoints` and
`connections`. This allows concurrent reads (receiver tasks, stats) while
the path command processor adds/removes entries. Methods change from
`&mut self` to `&self`.

New transport methods:
- `add_path(&self, path_id, bind_addr, peer_addr)` — creates endpoint,
  establishes QUIC connection, stores both.
- `remove_path(&self, path_id)` — closes connection with reason code,
  removes endpoint entry.
- `spawn_receiver_for_path()` — spawns a single receiver task for a
  newly added path (extracted from `spawn_receivers()`).

### 3. PathCommand channel

An `mpsc::channel<PathCommand>` connects the HTTP handler to the tunnel's
main runtime loop. The command processor task handles:

- **Add**: calls `transport.add_path()`, `scheduler.add_path()`,
  `stats.add_path()`, and spawns a receiver for the new path.
- **Remove**: calls `transport.remove_path()`, `scheduler.remove_path()`,
  `stats.remove_path()`.

### 4. Control messages

Two new control message variants (`PathAdd`, `PathRemove`) allow the remote
peer to be notified of path changes. Receivers handle these in the existing
control message dispatch.

## Alternatives Considered

1. **Full restart on path change**: Simple but causes data loss and session
   interruption. Unacceptable for real-time use cases.

2. **Shared mutable state with RwLock**: Works but DashMap provides better
   concurrent read throughput since receiver tasks and stats queries don't
   block each other.

3. **Signal-based approach (Unix signals)**: Platform-specific and limited
   to simple triggers — can't carry bind/peer address parameters.

## Consequences

- Paths can be added and removed without tunnel restart
- Zero downtime during network transitions (e.g., WiFi → cellular handoff)
- DashMap avoids lock contention between concurrent receiver tasks
- HTTP API enables automation and integration with network managers
