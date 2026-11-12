# ADR-0074: One metrics registry, rendered three ways; OTLP export and a flight recorder

## Status: Proposed (draft for the user's decision). Supersedes ADR-0013's Layer 4 (Prometheus) only; ADR-0013's Layers 1–3 stand.

## Context

The engine has two observability surfaces that share no source.
- The `/status` JSON (`monitor/stats.rs`, ADR-0013) is the only always-on
  one. Several of its fields are misleading:
  - `blocks.*`, `fec.actual_failure_rate` and `fec.pi_correction` have no
    writer on the window pipeline;
  - `rtt_ms` mixes the app-echo clock and the peer's reported average;
  - `cwnd` is Copa's in-app window in symbols, not the wire window;
  - `throughput_mbps` is the offered send rate.
- Some 40 tagged log lines (`[DIAG]`, `[LAT]`, `[HOLD]`, …) are mostly
  `RWM_*`-gated, and in most of them the counting itself is gated. The VM
  harness scores committed ledgers from them by regex.

Of the quantities this project scored or diagnosed in §21–§48b, most can be
read only from a gated log line, and several (wire RTT against app-echo RTT,
send-buffer dwell, ProbeRtt time, first-report hole age) from no always-on
source at all. `docs/derivations/observability-design.md` holds the
inventory, the field audit and the design.

The OpenTelemetry Rust SDK (0.33.1, read at source) gives no zero-cost record
path:
- `ValueMap::measure` takes an `RwLock` read and hashes the attribute slice.
  An attribute slice that is out of order is sorted into a fresh `Vec`
  (`internal/mod.rs:127–177`, `:427–435`).
- Histograms update under a `Mutex` (`internal/histogram.rs:19–45`).
- Bound instruments are behind `experimental_metrics_bound_instruments`.
- Exemplars are always empty (`internal/aggregate.rs:271, 303, 308`).
- `ResourceMetrics` cannot be built outside the crate (`data/mod.rs:13–17`).

## Decision

1. **One registry, per engine instance.**
   - A static, const-declared instrument table: name, kind, unit, clock,
     bounded attributes.
   - Storage hangs off `SharedStats`, so a loopback process with a client
     and a server keeps two registries.
   - Counters and gauges are atomics on the dense 64-path table. Histograms
     are fixed arrays of atomic buckets that are exactly OTel's exponential
     scale 3 (bounds 2^(i/8), indexed without `log` by a 7-entry mantissa
     table), so OTLP gets a native `ExponentialHistogram`.
   - Recording is one relaxed `fetch_add` (two for a histogram) and never
     allocates.
   - The only attributes are path index and role. δ, ρ and r are gauges,
     never labels; no hint name is ever an attribute.
2. **Three renderings of the one registry.**
   - `/status` (v2: corrected fields, every old key kept with its meaning
     documented);
   - the existing log tags, **byte-identical**, each formatter reading the
     registry;
   - an OTLP exporter behind cargo feature `otel` (off by default, wasm
     untouched). It builds OTLP data from a registry snapshot on its own
     collection tick through `opentelemetry-proto` types and posts it over
     OTLP/HTTP. The SDK's instruments are never called on a data path.
     Configuration is the standard `OTEL_*` environment, and an unsupported
     value fails loudly at start-up.
3. **Exact samples stay where a tag prints nearest-rank quantiles.** The
   registry histogram is fed by the same record call, and a test bounds the
   two to one bucket width.
4. **A flight recorder for per-symbol events.**
   - Fixed 16-byte records go into per-thread rings, with a deterministic
     sample by seq hash.
   - A dump fires on read-only anomaly detectors.
   - An offline converter writes OTLP traces, qlog and Perfetto.
   - Online OTel spans are kept for low-rate events only: transfer,
     recovery round, detector firing.
   - Exemplars point only at sampled seqs.
5. **Instruments never change a law.** No law, gate, threshold or emission
   decision reads the registry, a detector or the recorder; a test pins
   that. Detectors decide only what is dumped.

## Consequences

- Each always-on instrument has a priced cost: a ns bench, an allocation
  count per symbol, and in-process thread CPU or instructions per byte. The
  §41c guard cannot resolve sub-percent overhead, so it is a backstop, not
  the bound.
- The harness gains a machine-readable end-of-run snapshot. Committed
  ledgers stay re-scorable because the tags do not change, and a
  cross-check test reads both.
- With the `otel` feature on, a build pulls in `opentelemetry-proto`,
  `prost` and an HTTP client. VM build memory has to be measured before the
  feature enters any battery binary.
- `/status` consumers (`main.rs` `status` subcommand,
  `tests/monitoring_test.rs`) see corrected semantics; any rename is the
  user's call.
- The phased plan (design doc §9) is red-first. Each phase keeps every
  committed log format byte-identical.
