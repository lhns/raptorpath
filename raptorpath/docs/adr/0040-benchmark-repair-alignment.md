# ADR-0040: Benchmark Repair Alignment + Multi-Backend Comparison

## Status

Accepted

## Context

The transport comparison benchmark (ADR-0036) showed raptorpath with 38-50% overhead across all
scenarios, even on near-lossless datacenter links. This was misleading because the benchmark used
`FecRateController` (PI feedback controller) for window-mode repair rate computation, while
production window-mode (`run_window_sender`) uses a simple fractional `repair_debt` accumulator
(ADR-0037).

**Root cause:** The PI controller's integral term winds up during early batches (before the loss
estimator converges) and sustains elevated repair rates even when actual loss is near zero. This is
expected behavior for PI — the integral term provides robustness against model mismatch — but it
produces misleading benchmark numbers that don't reflect production behavior.

**PI controller stays in codebase:** The `FecRateController` with PI feedback remains in
`fec_rate.rs` and is used by block-mode `run_block_sender` where PI makes more sense (block decode
is binary success/failure). Only the benchmark was misusing it for window-mode repair.

Additionally, the benchmark only tested the RLC window backend. With five FEC backends available
(RLC, METTLE, RaptorQ, Reed-Solomon, Streaming), the benchmark should compare at least the three
most distinct: RLC (window, GF(2^8)), METTLE (window, XOR peeling), and RaptorQ (block, fountain).

## Decision

### 1. Speed up the benchmark

- Add `[profile.test] opt-level = 2` to the workspace `Cargo.toml`. RLC's GF(2^8) Gaussian
  elimination is 10-50x slower unoptimized.
- Reduce constants: `NUM_SYMBOLS = 2000` (was 4000), `NUM_TRIALS = 10` (was 20). With the
  fractional accumulator (no PI wind-up), 2000 symbols is sufficient for convergence. 10 trials
  is statistically sound for mean comparisons.

### 2. Replace PI controller with fractional accumulator

In `run_raptorpath_single()` and `run_raptorpath_dual()`, replace `FecRateController` with:

```rust
let mut repair_debt: f64 = 0.0;
// ...
let loss_rate = estimator.loss_rate();
repair_debt += batch_size as f64 * loss_rate * REPAIR_FACTOR;
while repair_debt >= 1.0 {
    repair_debt -= 1.0;
    let repair = encoder.generate_repair();
    channel.send(repair);
}
```

This matches the production `run_window_sender` accumulator (ADR-0037) with `REPAIR_FACTOR = 4.0`.

### 3. Add multi-backend testing

Parameterize window-mode runners with `FecBackend` to test both RLC and METTLE:

| Config | Backend | Mode | Architecture |
|--------|---------|------|-------------|
| `rp_rlc_single` | RLC | Window | GF(2^8) Gaussian elimination |
| `rp_rlc_dual` | RLC | Window | (dual-path multipath) |
| `rp_mettle_single` | METTLE | Window | XOR peeling decoder |
| `rp_mettle_dual` | METTLE | Window | (dual-path multipath) |

### 4. Add RaptorQ block-mode comparison

Add `run_raptorq_single()` — a separate block-mode runner using `RaptorqEncoder`/`RaptorqDecoder`
(RFC 6330 fountain code). Key differences from window-mode:

- Block assembly latency: symbols wait until a block of `BLOCK_SIZE = 50` symbols is full
- RaptorQ's near-optimal erasure recovery (~1% overhead) means efficient bandwidth usage
- Source symbols delivered in block-granularity chunks, not individually

Config: `rp_raptorq_single` — single path, block FEC.

### 5. Re-export RaptorQ types

Added `pub use raptorq_backend::{RaptorqEncoder, RaptorqDecoder}` to `fec/mod.rs` so integration
tests can access block-mode encoder/decoder.

## Expected Results

| Scenario | Loss rate | Expected overhead (window) | Expected overhead (RaptorQ) |
|----------|----------|---------------------------|---------------------------|
| dc_low_loss | ~0.1% | ~0.4% | ~0.4% |
| wifi_bursty | ~2% | ~8% | ~8% |
| lte_high_rtt | ~3% | ~12% | ~12% |
| lossy_satellite | ~8% | ~32% | ~32% |
| wifi_lte_asymmetric | ~1% | ~4% | ~4% |

Overhead now scales proportionally with `loss_rate * REPAIR_FACTOR` instead of the flat 38-50%
caused by PI integral wind-up.

## Consequences

- Benchmark results now reflect production behavior
- PI controller code unchanged — still used by block-mode production sender
- Multi-backend comparison reveals backend-specific tradeoffs (decode speed vs overhead)
- `[profile.test]` speeds up all test builds, not just this benchmark
- Total benchmark runtime should drop from ~2.5 hours to ~5-15 minutes
