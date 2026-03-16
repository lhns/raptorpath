# ADR 0030: Runtime FEC Backend Switching

## Status

Resolved

## Context

The FEC backend was fixed at startup via config (`--fec-backend` / TOML). All
blocks and the entire window session used the same backend for the connection's
lifetime. Channel conditions change over time — a path that starts at 0.1% loss
(RaptorQ-optimal) can degrade to 12% (where Mettle is better). Runtime switching
lets the sender adapt the codec to current conditions.

## Decision

Implement runtime FEC backend switching with:

### Protocol Changes (v2 → v3)
- Add `backend: FecBackend` field to `BlockStart` and `WindowStart` messages
- Add `WindowSwitch { flush_seq, new_backend, symbol_size }` (sender → receiver)
- Add `WindowSwitchAck { flush_seq }` (receiver → sender)
- Bump `PROTOCOL_VERSION` to 3

### BackendSelector Component
Loss-based heuristic with configurable thresholds and hysteresis:

**Block mode:**
| Condition | Backend |
|-----------|---------|
| loss < 0.01 | RaptorQ (near-MDS) |
| 0.01 ≤ loss < 0.10 | RLC (rateless, moderate-loss) |
| loss ≥ 0.10 | Mettle (fast XOR decode) |
| User forced `--fec-backend` | Honor override |

**Window mode** (only window-capable backends):
| Condition | Backend |
|-----------|---------|
| Bursty (GE burst > 3) | Streaming (delay-optimal) |
| loss < 0.01 | RLC |
| loss ≥ 0.10 | Mettle |

Hysteresis: minimum 5s between switches, condition must persist for 3 consecutive
evaluations (debounce).

### Block-Mode Switching
Per-block: `BackendSelector::evaluate()` called before each block encode. Receiver
creates decoder from `BlockStart.backend` (not fixed config).

### Window-Mode Switching
Coordinated flush protocol:
1. Sender stops adding sources, generates extra repair burst
2. Sends `WindowSwitch { flush_seq, new_backend, symbol_size }`
3. Receiver rebuilds decoder with new backend
4. Receiver sends `WindowSwitchAck { flush_seq }`
5. Sender rebuilds encoder, resumes

### Configuration
```toml
fec_switch_threshold_low = 0.01   # below → RaptorQ
fec_switch_threshold_high = 0.10  # above → Mettle
fec_switch_interval = 5           # minimum seconds between switches
fec_auto_switch = true            # false to disable
```

When `fec_backend` is explicitly set, auto-switching is disabled unless
`fec_auto_switch = true` is also set.

### Observability
- `fec.backend_switches` counter (AtomicU64)
- `fec.current_backend` gauge (AtomicU8)
- `info!` log on every switch with old/new backend and loss rate

## Consequences

- Sender adapts FEC codec to changing conditions without reconnection
- Receiver uses per-block/per-message backend → correct decoder always created
- Hysteresis prevents oscillation between backends at threshold boundaries
- Explicit `--fec-backend` still honored (no auto-switching)
- Protocol version bump (v3) breaks compatibility with v2 peers
- FecRateController overhead updated on switch via `update_backend()`
