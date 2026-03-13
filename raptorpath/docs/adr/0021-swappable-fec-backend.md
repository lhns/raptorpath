# ADR-0021: Swappable FEC Backend

## Status: Resolved

## Context

RaptorPath's FEC layer was hardcoded to RaptorQ (RFC 6330) throughout. The encoder
and decoder types were concrete structs used directly in `net/mod.rs`, `stream.rs`,
and all test files. This made it impossible to experiment with alternative erasure
codes without invasive changes across the codebase.

We wanted to evaluate METTLE (Multi-Edge Type with Touch-less Leading Edge), a
streaming SC-MET-LDGM code from Yu, Yang, Meng, Xu (Georgia Tech, arxiv 2602.10020,
2026). METTLE offers O(1) peeling decoding with GF(2)-only operations, potentially
lower latency than RaptorQ for streaming use cases, and decoding latency decoupled
from block size (depends on window size `w`, not total source symbols `k`).

## Decision

Introduce a trait-based FEC abstraction with a factory enum:

- **`FecEncoder` trait**: `source_symbols()` + `repair_symbols(count)` — any erasure
  code that produces systematic source symbols and on-demand repair symbols.
- **`FecDecoder` trait**: `add_symbol()` returning decoded data when complete, plus
  status methods (`is_decoded`, `total_fed`, `received_ids`, etc.).
- **`FecBackend` enum**: `RaptorQ` | `Mettle` — provides `create_encoder()` and
  `create_decoder()` factory methods. Defaults to `RaptorQ`.

The METTLE implementation lives in a standalone `mettle` crate within the workspace,
with its own README, tests, and benchmarks. The `mettle_backend` module in raptorpath
adapts it to the FecEncoder/FecDecoder traits.

The backend is exposed to users via:
- **CLI flag**: `--fec-backend mettle` (or `raptorq`)
- **Config file**: `fec_backend = "mettle"` in TOML
- Default is `raptorq`

## Consequences

### Positive

- **Pluggable FEC**: switching backends is a one-line config change
- **Independent testing**: the `mettle` crate has 60+ tests covering peeling mechanics,
  streaming behavior, small-window characteristics, and statistical evaluation
- **No wire protocol change**: `EncodingParams` and `WireSymbol` are codec-agnostic;
  repair symbol metadata (bin members) is encoded in the data field
- **Clean separation**: existing RaptorQ code unchanged in behavior, just wrapped in traits

### Negative

- **Trait object overhead**: `Box<dyn FecEncoder>` / `Box<dyn FecDecoder>` add one
  vtable indirection per call. Negligible compared to actual FEC computation.
- **METTLE small-window caveat**: the paper evaluates at w=600; raptorpath uses w≈50.
  Performance at small windows requires higher overhead factor (15% vs 10%) and more
  repair symbols for large blocks.
- **Repair wire format**: METTLE repair symbols carry bin membership lists in-band,
  adding ~4 bytes per member to each repair symbol. For l=4 edges this is ~16 bytes
  of overhead per repair symbol.

## Files Changed

| File | Change |
|------|--------|
| `src/fec/traits.rs` | New — FecEncoder/FecDecoder traits, FecBackend enum |
| `src/fec/raptorq_backend.rs` | Renamed from codec.rs, trait impls added |
| `src/fec/mettle_backend.rs` | New — adapts mettle crate to FecEncoder/FecDecoder |
| `src/fec/mod.rs` | Updated re-exports |
| `src/fec/stream.rs` | Uses trait objects instead of concrete types |
| `src/net/mod.rs` | Uses FecBackend factory for encoder/decoder creation |
| `tests/fec_codec.rs` | Tests both RaptorQ and Mettle backends |
| `benches/fec_bench.rs` | Benchmarks both backends |
| `mettle/` | New standalone crate (encoder, decoder, graph, gf2) |
