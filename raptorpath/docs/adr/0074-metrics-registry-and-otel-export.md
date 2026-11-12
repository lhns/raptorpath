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
     table, upper-inclusive), so OTLP gets a native `ExponentialHistogram`.
     Every instrument exports in the unit it records, with no rescaling.
   - Recording is a single-writer atomic store, or one relaxed `fetch_add`
     where a cell has two writers, plus the bucket index for a histogram.
     It never allocates.
   - The attribute set is closed:
     - the path index;
     - role and instance as resource attributes;
     - on tick-computed process and task instruments only, semconv's
       `cpu.mode` and `process.context_switch.type`, and a fixed task-name
       set.
   - Every other split is a separate instrument. δ, ρ and r are gauges,
     never labels; no hint name is ever an attribute.
2. **Three renderings of the one registry.**
   - `/status` (v2: corrected fields, every old key kept with its meaning
     documented);
   - the existing log tags, **byte-identical**, each formatter reading its
     own registry snapshot at its own cadence, so per-line values do not
     shift;
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
   - Fixed 16-byte records go into per-task rings, with a deterministic
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

## Decisions (user, 2026-11-12)

The status stays Proposed until the implementation passes its gates; these answer the design's open questions (`observability-design.md` §11). Q3–Q5 rationale is the coordinator's (the advisor was unavailable), to be confirmed at the next review.
- **Q1 — `/status` dead and misleading keys: removed.** `blocks.*`, `fec.actual_failure_rate`, `fec.pi_correction` go; `rtt_ms`, `throughput_mbps`, `cwnd`/`in_slow_start`, `jitter_us`, `symbols_received` are replaced by the catalogue's named instruments (the clock and unit in the name). The `status` CLI and `monitoring_test.rs` change with them.
- **Q2 — OTLP `http/protobuf`** (the spec default) for now.
- **Q3 — the flight recorder's sampling is a configurable ratio, like probabilistic tracing.** The deterministic seq-hash mask is the sampler (a `TraceIdRatioBased` analogue keyed on the symbol seq, so both ends and every recorder sample the same symbols); the ratio is one continuous parameter, read from the standard `OTEL_TRACES_SAMPLER=traceidratio` / `OTEL_TRACES_SAMPLER_ARG`. Default 1/64 always on; ratio 1 is the full-rate recorder.
- **Q4 — the 10 ms lag probe: decided by measurement, rule fixed now.** Phase P1 measures its cost (ns bench + in-process CPU, metrics on vs off). If ≤ 0.1 % of a core and no §41c clause moves, it is always on at 10 ms; otherwise always on at 100 ms, with 10 ms enabled only together with the full-rate recorder.
- **Q5 — collector tick 1 s, OTLP export 60 s** (the spec default `OTEL_METRIC_EXPORT_INTERVAL`, overridable by the standard variable). The harness reads the 1 s snapshot JSONL, never OTLP (battery rows are shorter than one export interval).
- **Q6 — Prometheus through an OpenTelemetry Collector** (OTLP in, Prometheus out). No native `/metrics` renderer: OTLP is the single export path, so a second renderer would be one more rendering to keep consistent. ADR-0013's Layer 4 is superseded.

### Amendments to Q3–Q5 (advisor review, same day)
- **Q3.**
  - **Variable gating:** `OTEL_TRACES_SAMPLER`'s spec default is `parentbased_always_on`. Unset here means 1/64 (a stated deviation), so a fleet-wide `always_on` must not silently switch on the full-rate ring. The variables are honoured only with the `otel` feature on; the resolved ratio is echoed at startup; the battery harness withholds `OTEL_*` (`env -u`).
  - **Mask form:** the mask is a threshold on a mixed hash of the seq, not `seq mod 2^k` (`RWM_EMIT_BURST` is 64, so low bits would alias with burst position). A threshold nests, so ends with different ratios still share samples.
  - **Registrations:** every pre-registration pins the ratio.
- **Q4.** The probe gets its own period setting (default by the rule below), not keyed on "ratio = 1". The rule's test cannot be "no §41c clause moves": `lag_E` may come from this probe (check whether it is the existing `[LAG]` probe under `RWM_RTOBS`, on in both arms of every battery so far), and the guard is blind below ≈ 5 % CPU. The test becomes: park counts, the probe's own in-process CPU, and the goodput clauses, probe on vs off. The 100 ms fallback is the coordinator's addition to the user's "only with the full-rate recorder", labelled as such.
- **Q5.**
  - **Final flush:** OTLP is rendered in-process, so the exporter must flush on shutdown, or a 9–50 s run exports nothing.
  - **Time-in-state counters:** share-of-time readouts (ProbeRtt, `paused`, qcwnd below BDP) are time-in-state counters, not tick-sampled gauges (a 9 s row gives ≈ 8 ticks). A joint condition such as R2b gets its own counter.
- **Q1.** Before removing keys, grep all consumers beyond the `status` CLI and `monitoring_test.rs`.
