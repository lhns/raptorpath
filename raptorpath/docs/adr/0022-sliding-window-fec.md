# ADR-0022: Sliding Window FEC Architecture

## Status: Resolved

## Context

Raptorpath's FEC layer is block-based: data accumulates in a buffer until a block is
full or a timeout fires, then the entire block is encoded, interleaved, and transmitted.
This introduces latency proportional to block assembly time — problematic for real-time
traffic (VoIP, gaming) where per-packet latency matters more than throughput.

Sliding-window FEC eliminates block boundaries: source symbols are sent immediately as
they arrive, and repair symbols are generated over a moving window of recent source
symbols. This gives near-zero encoding latency while maintaining erasure protection.

## Decision

### Algorithm-Driven Pipeline Selection

The pipeline shape (block vs window) is determined by the **algorithm's capabilities**,
not a user-chosen mode. Each FEC backend declares whether it is streaming-native via
`FecBackend::is_streaming()`:

**Block-only algorithms** (require all k sources upfront):
- **RaptorQ** — LDPC pre-coding needs the full block for GE
- **Reed-Solomon** — MDS code with fixed (k, n)

**Streaming-native algorithms** (operate incrementally):
- **RLC** — random linear combinations over GF(2^8), incremental GE decoder
- **METTLE** — graph-based spatial coupling, pure peeling (XOR-only) decoder

Pipeline selection (`is_window_mode`):
- `ProtocolHint::Realtime` + streaming backend → sliding-window pipeline
- Everything else → block-based pipeline

```rust
fn is_window_mode(hint: ProtocolHint, backend: FecBackend) -> bool {
    hint == ProtocolHint::Realtime && backend.is_streaming()
}
```

### New Traits

```rust
trait WindowEncoder: Send {
    fn add_source(&mut self, data: &[u8]) -> WireSymbol;
    fn generate_repair(&mut self) -> WireSymbol;
    fn window_span(&self) -> (u64, u64);
    fn advance(&mut self, oldest_seq: u64);
    fn window_size(&self) -> usize;
}

trait WindowDecoder: Send + Sync {
    fn add_symbol(&mut self, symbol: &WireSymbol) -> Vec<(u64, Bytes)>;
    fn advance(&mut self, oldest_seq: u64);
    fn total_fed(&self) -> u64;
}
```

These **extend** the existing `FecEncoder`/`FecDecoder` traits — they don't replace them.
Block-based backends continue using the current traits unchanged.

### Window Backend Implementations

**RLC (`RlcWindowEncoder`/`RlcWindowDecoder`)**:
- Encoder: maintains window of padded sources, generates repair as random linear
  combinations over GF(2^8) with deterministic PRNG coefficients
- Decoder: incremental Gaussian elimination with pivot table and cascade recovery
- Wire repair format: `[window_start(8)][window_count(2)][repair_index(4)][coded_data]`

**METTLE (`MettleWindowEncoder`/`MettleWindowDecoder`)**:
- Encoder: wraps `mettle::MettleEncoder` directly (already accepts sources incrementally),
  rebuilds on window advance (O(window_size) XOR ops)
- Decoder: sparse reimplementation of METTLE peeling over `BTreeMap<u64, Vec<u8>>` keyed
  by global sequence numbers (avoids `num_source` constructor requirement)
- Wire repair format: `[window_start(8)][num_members(2)][member_offsets: u16...][xor_data]`
- Key advantage: peeling is XOR-only (no GF(2^8) multiply), O(1) per recovery step

The sender creates `Box<dyn WindowEncoder>` and receiver creates `Box<dyn WindowDecoder>`
based on the configured `FecBackend`.

### Sender Pipeline

The existing block-mode sender loop is **preserved unchanged**. Window mode adds a
**parallel code path** selected by `is_window_mode(hint, backend)`:

```
Block mode:  TUN read → block_buf accumulation → flush → FecEncoder → interleave → send
Window mode: TUN read → frame_window_packet() → WindowEncoder::add_source() → send immediately
                        periodically → generate_repair() → send
                        on ACK → advance window
```

Window-mode specifics:
- Bypasses `block_buf` accumulation, `flush_timeout`, and `InterleavingBuffer`
- Source symbols get global sequence numbers (u64), not per-block `payload_id`
- Repair generation driven by packet count, rate set by `compute_repair_rate()`
- Window advanced based on `WindowAck` from receiver (shared `AtomicU64`)
- Window capped at `MAX_WINDOW_SIZE` (200 symbols) to bound memory
- **Multipath path selection**: source symbols sent on lowest-loss path,
  repair symbols on second-lowest-loss path for diversity (`select_window_paths()`)

### Receiver Pipeline

The receiver handles **both** block-mode and window-mode symbols. When `is_window_mode`
is true, a single long-lived `Box<dyn WindowDecoder>` is created at startup:

```
Block mode:   receive → lookup decoder by block_id (DashMap) → add_symbol() → extract_packets()
Window mode:  receive → WindowDecoder → reorder buffer → extract_window_packet() → TUN inject
```

**Reorder buffer**: recovered symbols pass through a `ReorderBuffer` that delivers them
in sequence order. Entries held longer than `reorder_timeout` (default 20ms) are
force-delivered to avoid head-of-line blocking. Max 500 buffered entries.

**WindowNack gap reporting**: the receiver tracks received sequences and periodically
sends `WindowNack { gaps: Vec<(u64, u64)> }` with up to 20 inclusive gap ranges of
missing sequences. This enables the sender to generate targeted repair.

### Wire Protocol

- Source symbols: `block_id` = global seq, `payload_id` = 0, `is_repair` = false
- RLC repair: `block_id` = window_end_seq, data = `[window_start(8)][window_count(2)][repair_index(4)][coded]`
- METTLE repair: `block_id` = window_end_seq, data = `[window_start(8)][num_members(2)][offsets: u16...][coded]`
- Control messages:
  - `WindowStart { symbol_size: u16 }` — sender announces window mode
  - `WindowAck { received_up_to: u64 }` — receiver reports delivery progress
  - `WindowNack { gaps: Vec<(u64, u64)> }` — receiver reports missing seq ranges

### Decoder Algorithms

**RLC — Incremental Gaussian Elimination**:
- Source symbols are recovered directly
- Repair symbols are reduced against recovered sources and existing pivot rows
- When a row reduces to a single unknown, the source is recovered
- Recovery cascades: each newly recovered source triggers further reductions
- This enables recovery even when all sources are lost (repair-only recovery)

**METTLE — Pure Peeling**:
- Source symbols are stored directly and propagated to pending bins
- Repair symbols are parsed, known sources XOR'd out, and inserted as pending bins
- Degree-1 bins immediately yield a recovery; cascade propagates to other bins
- XOR-only: no field multiplication, O(1) per symbol recovery step
- METTLE's TLE (Touch-less Leading Edge) guarantees peeling start points

## Consequences

### Positive

- **Zero encoding latency**: source packets sent immediately, no block assembly wait
- **Natural burst resilience**: window spreads repair across time without explicit interleaving
- **Additive change**: block pipeline untouched, window pipeline added alongside
- **Algorithm-driven design**: `is_streaming()` determines pipeline shape automatically
- **Two streaming backends**: RLC (standards-based GE) and METTLE (efficient peeling)
- **Ordered delivery**: reorder buffer ensures TCP-over-tunnel correctness
- **NACK-based repair**: gap reporting enables targeted retransmission
- **Path-aware scheduling**: source on best path, repair on diverse path

### Negative

- **Two code paths**: block and window pipelines coexist, increasing maintenance surface
- **Memory**: window decoder maintains O(window_size × symbol_size) state continuously,
  plus reorder buffer overhead
- **METTLE encoder rebuild**: window advance requires rebuilding the encoder (O(w) XOR ops)

## Implementation Checklist

- [x] GF(2^8) finite field arithmetic (`src/fec/gf256.rs`)
- [x] Reed-Solomon backend (`src/fec/rs_backend.rs`)
- [x] RLC block-mode backend (`src/fec/rlc_backend.rs`)
- [x] `WindowEncoder`/`WindowDecoder` traits (`src/fec/window_traits.rs`)
- [x] RLC window encoder/decoder with incremental GE (`src/fec/rlc_window.rs`)
- [x] METTLE window encoder/decoder with peeling (`src/fec/mettle_window.rs`)
- [x] `FecBackend::is_streaming()` — algorithm-driven pipeline selection
- [x] Window-mode framing (`frame_window_packet`/`extract_window_packet` in `src/net/framing.rs`)
- [x] `WindowStart`/`WindowAck`/`WindowNack` control messages (`src/transport/protocol.rs`)
- [x] `compute_repair_rate()` (`src/control/fec_rate.rs`)
- [x] Window-mode sender loop with `Box<dyn WindowEncoder>` (`src/net/mod.rs`)
- [x] Window-mode receiver with `Box<dyn WindowDecoder>` (`src/net/mod.rs`)
- [x] Reorder buffer for ordered delivery (`ReorderBuffer` in `src/net/mod.rs`)
- [x] WindowNack gap reporting (`compute_gap_ranges` in `src/net/mod.rs`)
- [x] Multipath repair scheduling (`select_window_paths` in `src/net/mod.rs`)
- [x] Integration tests for both RLC and METTLE (`tests/fec_window_test.rs`)
- [x] Window-mode benchmarks: encode, decode, loss recovery (`benches/fec_bench.rs`)

## Files

| File | Role |
|------|------|
| `src/fec/traits.rs` | `FecBackend::is_streaming()` |
| `src/fec/window_traits.rs` | WindowEncoder/WindowDecoder traits |
| `src/fec/rlc_window.rs` | RLC sliding window encoder/decoder |
| `src/fec/mettle_window.rs` | METTLE sliding window encoder/decoder |
| `src/fec/gf256.rs` | GF(2^8) finite field arithmetic |
| `src/fec/rs_backend.rs` | Reed-Solomon backend |
| `src/fec/rlc_backend.rs` | RLC block-mode backend |
| `src/fec/mettle_backend.rs` | METTLE block-mode backend |
| `src/net/mod.rs` | Pipeline: is_window_mode, sender, receiver, reorder buffer, gap reporting |
| `src/net/framing.rs` | Per-symbol framing functions |
| `src/control/fec_rate.rs` | `compute_repair_rate()` |
| `src/transport/protocol.rs` | WindowStart/WindowAck/WindowNack messages |
| `tests/fec_window_test.rs` | Integration tests (RLC + METTLE) |
| `benches/fec_bench.rs` | Block + window benchmarks |
