# ADR-0049: Benchmark Dashboard Generator

## Status

Accepted

## Context

The matrix benchmark (ADR-0045) produces JSON files with 10 metrics across
168 cells per run, accumulating across commits. Without visualization, comparing
results requires manual JSON diffing or reading markdown tables — neither scales
beyond a handful of runs.

We need an interactive dashboard that:
- Visualizes trends, heatmaps, and comparisons across all benchmark dimensions
- Produces a single self-contained HTML artifact (no build step, git-friendly)
- Works offline after initial CDN load
- Requires no pip dependencies

## Decision

### Architecture: single-file HTML with embedded data

`tools/generate_dashboard.py` (stdlib-only Python) reads all
`docs/benchmark-results-*.json` files and produces a single
`docs/benchmark-dashboard.html` with data embedded as a JS `const`.

The HTML uses two CDN dependencies:
- **Plotly.js 2.35** — charts (scatter, heatmap, bar)
- **Lit 3** — reactive web components

Three custom elements provide component encapsulation via Shadow DOM:

| Element | Role |
|---------|------|
| `<bench-dashboard>` | Root: state management, chart rendering, legend sync |
| `<bench-controls>` | Header, tab bar, filter radio groups, commit info display |
| `<bench-timeline>` | Clickable dot timeline for run selection |

### Four visualization tabs

1. **Trend** — ISO date x-axis with proportional time spacing, one chart per
   metric. Shows metric evolution across builds for a selected
   scenario/config/paths. Error bars show 95% CI. Tick format: `%b %d %H:%M`.

2. **Matrix** — one heatmap per metric for a selected commit. Backends on
   y-axis, scenarios on x-axis. Cell annotations show numeric values.
   Color scale: green=good, red=bad (reversed for metrics where lower=better).

3. **Loss Sweep** — two charts (uniform + Gilbert-Elliott bursty). Recovery
   rate vs loss percentage per backend. Sourced from `table1_uniform` and
   `table1b_bursty` JSON keys.

4. **Ablation** — bar chart per metric comparing baseline vs no_nack vs
   no_reorder vs no_pi for a selected backend/scenario/paths.

### Interactive controls

Filter visibility varies by tab:

| Control | Trend | Matrix | Sweep | Ablation |
|---------|-------|--------|-------|----------|
| Scenario | yes | — | — | yes |
| Config | yes | yes | — | — |
| Paths | yes | yes | — | yes |
| Backend | — | — | — | yes |
| Timeline | — | yes | yes | yes |

Clicking any data point shows commit provenance (hash, message, timestamp)
in the sticky header bar.

### Data format

Each benchmark JSON has top-level keys:

```
commit_hash, commit_message, timestamp,
matrix[],          — 168 entries (backend × scenario × config × paths)
table1_uniform[],  — loss sweep (uniform model)
table1b_bursty[]   — loss sweep (Gilbert-Elliott model)
```

Matrix entry: `{ backend, scenario, config, paths, metrics: { <name>: { mean, stddev, ci95 } } }`

Sweep entry: `{ backend, loss_pct, recovery: { mean, stddev, ci95 } }`

10 stable metrics: throughput_mbps, recovery_rate, overhead_pct,
total_repair_count, p50/p95/p99_latency_ms, deadline_miss_pct,
in_order_rate, tail_drops.

### Design decisions and tradeoffs

1. **Shadow DOM + Plotly CSS** — Lit components use Shadow DOM for
   encapsulation, but Plotly injects its modebar/SVG styles into
   `document.head` where they cannot penetrate the shadow boundary. The
   dashboard manually replicates modebar CSS and SVG pointer-events rules
   inside the component's `static styles`.

2. **`Plotly.react()` over `Plotly.newPlot()`** — incremental updates on
   filter/tab changes avoid tearing down and recreating the entire chart DOM.
   Event listeners are bound once per div (tracked via `_bindedDivs` Set).

3. **Legend sync** — clicking a backend legend item in any chart hides/shows
   that backend across all charts in the same tab. Implemented via a shared
   `_hiddenBackends` Set and `Plotly.restyle` calls on sibling divs. The
   `plotly_legendclick` handler returns `false` to suppress Plotly's default
   single-chart toggle.

4. **`requestAnimationFrame` batching** — `updated()` defers chart rendering
   to the next animation frame via `this.updateComplete.then(() =>
   requestAnimationFrame(...))`, preventing layout thrashing when multiple
   reactive properties change in the same microtask.

5. **commitInfo skip** — `updated()` short-circuits when the only changed
   property is `commitInfo`, avoiding a full chart re-render for what is
   purely a header text update.

6. **CDN dependencies** — Plotly.js (~3 MB) and Lit are loaded from CDN.
   Tradeoff: no true offline-first, but avoids embedding megabytes into the
   HTML file. After initial load, browser cache provides offline access.

7. **Reversed color scales** — metrics where lower=better (overhead_pct,
   deadline_miss_pct, latencies, tail_drops, total_repair_count) use a
   green→yellow→red scale; higher-is-better metrics use red→yellow→green.

### Usage

```bash
# Generate the dashboard from benchmark JSON files
cd raptorpath && python tools/generate_dashboard.py

# Open in browser
start docs/benchmark-dashboard.html   # Windows
open docs/benchmark-dashboard.html    # macOS
```

Prerequisites: Python 3 (stdlib only, no pip install needed). Benchmark JSON
files must exist in `docs/`.

### Adding new benchmark data

1. Run `cargo test --release bench_suite` to produce a new JSON file in `docs/`
2. Re-run `python tools/generate_dashboard.py` to rebuild the dashboard with
   the new run included

## Consequences

- All benchmark data is now explorable in a single interactive artifact
- Commit-to-commit regressions are visually obvious in the Trend tab
- The heatmap Matrix tab surfaces backend × scenario weak spots at a glance
- Shadow DOM workarounds add ~50 lines of CSS that must be kept in sync if
  Plotly changes its internal markup (low risk — Plotly's DOM is stable)
- CDN dependency means first load requires internet; subsequent loads use
  browser cache
- The generated HTML file is large (proportional to number of JSON runs) but
  compresses well with gzip and is excluded from code review diffs
