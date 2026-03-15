# METTLE — Streaming Erasure Code with Peeling Decoder

Research implementation of the METTLE (**M**ulti-**E**dge **T**ype with **T**ouch-less **L**eading
**E**dge) streaming erasure code.

Based on: Yu, Yang, Meng, Xu (Georgia Tech),
*"Efficient Streaming Erasure Code with Peeling Decodability,"* arxiv 2602.10020, February 2026.

## Patent Notice

The METTLE scheme is covered by a provisional patent filed by the original authors.
**This implementation is for research and evaluation purposes only.**

## What is METTLE?

METTLE is an SC-MET-LDGM (spatially-coupled multi-edge-type LDGM) erasure code designed for
streaming applications. Unlike block codes (RaptorQ, Reed-Solomon), METTLE operates over a
sliding window and decodes symbols on-the-fly as they arrive.

### Key properties

- **Pure peeling decoder** — no Gaussian elimination fallback. O(1) per symbol.
- **Streaming-native** — decoding latency depends on window size `w`, not block size `k`.
- **GF(2) operations only** — all coding is packet-level XOR. No field multiplication.
- **Systematic** — source packets are sent first, unmodified. Zero cost on lossless links.

### How it works

1. **Encoding**: each source packet at position `x` is XOR'd into `l` bins (default 4).
   Bin positions are determined by a hash-based Tanner graph:
   - **Edge 1 (TLE)**: deterministic at `floor((1+c) * x)`. No two TLE edges collide.
   - **Edges 2..l**: stochastic, drawn from binomial distributions that place edges
     progressively closer to the source position.

2. **Decoding (peeling)**:
   - When a bin has degree 1 (only one unknown source packet), recover it via XOR.
   - XOR the recovered packet out of all other bins containing it.
   - This may create new degree-1 bins → cascade continues.
   - The TLE edge guarantees peeling always has a starting point.

```
Source:  [P0] [P1] [P2] [P3] [P4] ...
            \   |   / \   |  /
             v  v  v   v  v v
Bins:    [B0] [B1] [B2] [B3] [B4] [B5] ...
              (XOR of contributing source packets)

Peeling: B0 has degree 1 → recover P0
         XOR P0 out of B1, B2 → B1 drops to degree 1
         Recover P1 from B1 → cascade continues...
```

## Usage

```rust
use mettle::{MettleEncoder, MettleDecoder, MettleConfig};

let config = MettleConfig::default();  // w=600, l=4, c=0.1
let seed = 42u64;

// Encode
let mut encoder = MettleEncoder::new(config, seed);
let data: Vec<Vec<u8>> = (0..100).map(|i| vec![i as u8; 1200]).collect();
for pkt in &data {
    encoder.add_source_packet(pkt);
}
let source = encoder.source_packets();  // systematic: unmodified
let coded = encoder.coded_packets();    // XOR bins for repair

// Decode (simulate 5% loss)
let mut decoder = MettleDecoder::new(config, data.len(), seed);
for (i, pkt) in source.iter().enumerate() {
    if i % 20 != 0 {  // drop every 20th
        decoder.add_source_packet(i, pkt);
    }
}
for cp in &coded {
    decoder.add_coded_packet(cp);
    if decoder.is_complete() { break; }
}
let recovered = decoder.recovered_data().unwrap();
```

## Configuration

| Parameter | Field | Default | Description |
|-----------|-------|---------|-------------|
| Window size | `window_size` | 600 | How far forward each source packet's edges can reach. Larger = better efficiency, higher latency. |
| Edges | `num_edges` | 4 | Number of bins each source packet is XOR'd into. More edges = better protection, more computation. |
| Overhead | `overhead_factor` | 0.1 | Rate overhead: produces `(1+c)` coded symbols per source. Higher = more redundancy. |

### Presets

- `MettleConfig::default()` — paper's configuration: w=600, l=4, c=0.1
- `MettleConfig::small_window()` — tuned for raptorpath: w=50, l=4, c=0.15

## Performance characteristics

From the paper (w=600):

| Metric | METTLE | RaptorQ (streaming) |
|--------|--------|---------------------|
| Decode time/packet | 2.6 μs | 124-220 μs |
| Operations | 3-5 XOR per packet | GF(2^8) Gaussian elim |
| Overhead @ 1% loss | 5.5% | 6.14% |
| Overhead @ 10% loss | 25% | 15% |

**Small window note**: the paper optimized for w=600. At w=50 (raptorpath's target),
recovery still reaches 100% at up to 10% loss when all coded bins are available.
Run the `small_window` and `statistical` tests to verify at your target window size.

## Known issues and fixes (ADR 0028)

An edge probability off-by-one bug was discovered and fixed in March 2026. The
stochastic edge formula used `p = 1/2^(i-1)` instead of `p = 1/2^i`, causing
the first stochastic edge to always collide with the TLE edge. After the
encoder's deduplication, this wasted 25% of graph connectivity and severely
degraded recovery rates (0-70% instead of 100%).

**Impact**: the fix changed recovery from near-zero to 100% across all tested
configurations (w=50 and w=600, 1-10% loss).

Regression tests in `graph.rs` (inline) and `tests/edge_analysis.rs` guard
against reintroduction. Key invariants:

- First stochastic edge probability must be < 1.0 (p = 0.5, not 1.0)
- TLE-stochastic collision rate must be < 5% (was ~100% with the bug)
- Average unique edges per source must be > 3.5 out of 4
- Mean offsets must follow geometric spacing: n/2, n/4, n/8

### Integration notes (raptorpath adapter)

When using METTLE through raptorpath's `FecBackend::Mettle` block adapter:

- **All coded bins must be sent.** METTLE is a fixed-rate code; the peeling
  decoder needs the complete graph structure to cascade. Unlike rateless codes
  (RaptorQ, RLC), partial bin sets break recovery. The `repair_symbols()` method
  returns all bins regardless of the `count` parameter.
- **`num_source` must match the data split**, not the application-level packet count.
  The decoder computes `num_source = ceil(transfer_length / symbol_size)`.
- **Window mode repair selection** uses golden-ratio stride to distribute repairs
  across the bin range, avoiding clustering at early positions.

## Testing

```bash
# Unit tests (includes edge probability regression tests)
cargo test -p mettle

# Integration tests with output
cargo test -p mettle -- --nocapture

# Edge analysis: collision rate, mean offsets, A/B test (ADR 0028)
cargo test -p mettle --test edge_analysis -- --nocapture

# Small window characterization (prints statistics)
cargo test -p mettle small_window -- --nocapture

# Statistical evaluation (500 trials per configuration)
cargo test -p mettle statistical -- --nocapture

# Benchmarks
cargo bench -p mettle
```

## Architecture

```
mettle/
├── src/
│   ├── lib.rs          # Public API, MettleConfig
│   ├── encoder.rs      # Streaming encoder (XOR into bins)
│   ├── decoder.rs      # Peeling decoder (cascade recovery)
│   ├── graph.rs        # Tanner graph edge generation (TLE + binomial)
│   └── gf2.rs          # GF(2) XOR operations
├── tests/
│   ├── edge_analysis.rs   # Edge collision & probability regression tests (ADR 0028)
│   ├── encode_decode.rs   # Full round-trip tests
│   ├── peeling.rs         # Peeling mechanics tests
│   ├── streaming.rs       # Streaming encode/decode tests
│   ├── small_window.rs    # Small window characterization
│   └── statistical.rs     # Large-scale statistical evaluation
└── benches/
    └── mettle_bench.rs   # Criterion benchmarks
```

## References

- Yu, Yang, Meng, Xu. "Efficient Streaming Erasure Code with Peeling Decodability."
  arxiv 2602.10020, February 2026.
- Luby. "LT Codes." FOCS 2002. (Foundation for peeling decoders)
- Shokrollahi. "Raptor Codes." IEEE Trans. IT, 2006. (Comparison point)
