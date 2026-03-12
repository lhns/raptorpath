# ADR-0015: No Graceful Shutdown

## Status
**Resolved** — Ctrl+C triggers broadcast shutdown signal. Sender flushes partial block, sends ControlMessage::Shutdown to peer, exits cleanly. Receiver handles shutdown signal and Shutdown messages. TunInterface Drop impl logs cleanup.

## Context
The main loop uses `tokio::select!` on sender and receiver handles. On Ctrl+C or process termination, tasks are aborted immediately.

## Problem
1. **Partial blocks lost**: data in `block_buf` that hasn't been encoded/sent yet is discarded
2. **In-flight blocks lost**: blocks that are partially received by the peer won't complete
3. **No drain signal**: peer doesn't know we're shutting down, keeps sending data into the void
4. **TUN not cleaned up**: on Windows, the wintun adapter may persist after crash
5. **QUIC connections not closed cleanly**: peer sees connection reset, not graceful close

## Decision Required
Implement signal-aware shutdown:

```rust
let (shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel(1);

// Handle Ctrl+C
tokio::spawn(async move {
    tokio::signal::ctrl_c().await.unwrap();
    info!("shutting down...");
    shutdown_tx.send(()).unwrap();
});

// In sender task:
tokio::select! {
    packet = tun.read_packet() => { ... }
    _ = shutdown_rx.recv() => {
        // Flush partial block
        if !block_buf.is_empty() { encode_and_send(block_buf); }
        // Send drain signal to peer
        transport.send_control(ControlMessage::Shutdown).await;
        break;
    }
}
```

### On Windows: cleanup adapter
```rust
impl Drop for TunInterface {
    fn drop(&mut self) {
        // wintun adapter is cleaned up when session/adapter are dropped
        // but log it for visibility
        info!("cleaning up TUN interface {}", self.name);
    }
}
```

## Consequences
- No data loss on clean shutdown
- Peer gets notified and can clean up
- Adapter resources are released
