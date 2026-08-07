#!/usr/bin/env python3
"""Generate an interactive benchmark dashboard from JSON result files.

Reads all docs/benchmark-results-*.json files and produces a single
self-contained docs/benchmark-dashboard.html with embedded Plotly.js charts.

No pip dependencies required — stdlib only.
"""

import glob
import json
import os
from datetime import datetime

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
DOCS_DIR = os.path.join(os.path.dirname(SCRIPT_DIR), "docs")


def load_runs():
    pattern = os.path.join(DOCS_DIR, "benchmark-results-*.json")
    files = sorted(glob.glob(pattern))
    if not files:
        raise SystemExit(f"No benchmark JSON files found matching {pattern}")
    runs = []
    for f in files:
        with open(f) as fh:
            runs.append(json.load(fh))
    print(f"Loaded {len(runs)} benchmark runs")
    return runs


HTML_TEMPLATE = r"""<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Raptorpath Benchmark Dashboard</title>
<script src="https://cdn.plot.ly/plotly-2.35.2.min.js"></script>
<style>
:root {
  --bg: #1a1a2e;
  --surface: #16213e;
  --border: #0f3460;
  --text: #e0e0e0;
  --text-dim: #8899aa;
  --accent: #e94560;
  --accent2: #0f3460;
}
* { box-sizing: border-box; margin: 0; padding: 0; }
body {
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
  background: var(--bg);
  color: var(--text);
  min-height: 100vh;
}
</style>
</head>
<body>

<bench-dashboard></bench-dashboard>

<script type="module">
import {LitElement, html, css} from 'https://cdn.jsdelivr.net/npm/lit@3/+esm';

// --- Embedded data ---
const RUNS = __DATA__;

// --- Helpers ---
const LATEST = RUNS[RUNS.length - 1];
const ALL_BACKENDS = [...new Set(LATEST.matrix.map(e => e.backend))].sort();
const ALL_SCENARIOS = [...new Set(LATEST.matrix.map(e => e.scenario))].sort();
const ALL_CONFIGS = [...new Set(LATEST.matrix.map(e => e.config))].sort();
const ALL_PATHS = [...new Set(LATEST.matrix.map(e => e.paths))].sort((a,b) => a-b);
const ALL_METRICS = Object.keys(LATEST.matrix[0].metrics).sort();

const BACKEND_COLORS = {
  'RaptorQ': '#2196F3',
  'ReedSolomon': '#4CAF50',
  'RLC': '#FF9800',
  'Streaming': '#9C27B0',
  'Retransmit': '#00BCD4'
};

const REVERSED_METRICS = new Set(['overhead_pct', 'deadline_miss_pct', 'p50_latency_ms',
  'p95_latency_ms', 'p99_latency_ms', 'tail_drops', 'total_repair_count']);

const PLOT_LAYOUT = {
  paper_bgcolor: '#16213e',
  plot_bgcolor: '#1a1a2e',
  font: { color: '#e0e0e0', size: 12 },
  margin: { t: 40, r: 30, b: 60, l: 70 },
  xaxis: { gridcolor: '#0f3460', zerolinecolor: '#0f3460' },
  yaxis: { gridcolor: '#0f3460', zerolinecolor: '#0f3460' },
  modebar: { bgcolor: 'transparent', color: '#8899aa', activecolor: '#e94560' },
};

const PLOT_CONFIG = {
  responsive: true,
  modeBarButtonsToRemove: ['lasso2d', 'select2d'],
};

function fmtTimestamp(ts) {
  const parts = ts.split('-');
  const hhmm = parts[3].substring(0, 2) + ':' + parts[3].substring(2, 4);
  return `${parts[0]}-${parts[1]}-${parts[2]} ${hhmm}`;
}

function fmtTimestampFull(ts) {
  const parts = ts.split('-');
  const time = parts[3].substring(0,2) + ':' + parts[3].substring(2,4) + ':' + parts[3].substring(4,6);
  return `${parts[0]}-${parts[1]}-${parts[2]} ${time}`;
}

function parseTimestamp(ts) {
  const p = ts.split('-');
  return `${p[0]}-${p[1]}-${p[2]}T${p[3].substring(0,2)}:${p[3].substring(2,4)}:${p[3].substring(4,6)}`;
}

// =====================================================
// <bench-controls>
// =====================================================
// =====================================================
// <bench-timeline>
// =====================================================
class BenchTimeline extends LitElement {
  static properties = {
    selectedIdx: { type: Number },
    visible: { type: Boolean },
  };

  static styles = css`
    :host { display: block; }
    :host([hidden]) { display: none; }
    .timeline {
      padding: 8px 24px;
      background: #131b30;
      border-bottom: 1px solid #0f3460;
      display: flex;
      align-items: center;
      gap: 6px;
      overflow: visible;
    }
    .tl-label {
      font-size: 0.75rem;
      color: #8899aa;
      text-transform: uppercase;
      letter-spacing: 0.5px;
      font-weight: 600;
      white-space: nowrap;
      margin-right: 4px;
    }
    .tl-track {
      display: flex;
      align-items: center;
      gap: 0;
      flex: 1;
      min-width: 0;
    }
    .tl-segment {
      height: 2px;
      flex: 1;
      background: #0f3460;
      min-width: 8px;
    }
    .tl-dot {
      width: 14px;
      height: 14px;
      border-radius: 50%;
      background: #0f3460;
      border: 2px solid #8899aa;
      cursor: pointer;
      flex-shrink: 0;
      transition: all 0.15s;
      position: relative;
    }
    .tl-dot:hover {
      background: #e94560;
      border-color: #e94560;
      transform: scale(1.3);
    }
    .tl-dot.active {
      background: #e94560;
      border-color: #e94560;
      transform: scale(1.3);
      box-shadow: 0 0 8px rgba(233,69,96,0.5);
    }
    .tl-dot .tooltip {
      display: none;
      position: absolute;
      top: 22px;
      left: 50%;
      transform: translateX(-50%);
      background: #1a1a2e;
      border: 1px solid #0f3460;
      border-radius: 4px;
      padding: 4px 8px;
      font-size: 0.72rem;
      color: #e0e0e0;
      min-width: 200px;
      white-space: nowrap;
      z-index: 200;
      pointer-events: none;
    }
    .tl-dot:hover .tooltip { display: block; }
  `;

  render() {
    if (!this.visible) return html``;
    const items = [];
    for (let i = 0; i < RUNS.length; i++) {
      if (i > 0) items.push(html`<div class="tl-segment"></div>`);
      const run = RUNS[i];
      const hash = run.commit_hash.substring(0, 7);
      const msg = (run.commit_message || '').substring(0, 60);
      const ts = fmtTimestamp(run.timestamp);
      items.push(html`
        <div class="tl-dot ${i === this.selectedIdx ? 'active' : ''}"
          @click=${() => this._select(i)}>
          <div class="tooltip">${hash} — ${msg}${'\n'}${ts}</div>
        </div>
      `);
    }
    return html`
      <div class="timeline">
        <span class="tl-label">History</span>
        <div class="tl-track">${items}</div>
      </div>
    `;
  }

  _select(idx) {
    this.dispatchEvent(new CustomEvent('run-change', { detail: idx, bubbles: true, composed: true }));
  }
}
customElements.define('bench-timeline', BenchTimeline);

class BenchControls extends LitElement {
  static properties = {
    scenario: { type: String },
    config: { type: String },
    paths: { type: Number },
    ablationBackend: { type: String },
    activeTab: { type: String },
    commitInfo: { type: Object },
    selectedRunIdx: { type: Number },
  };

  static styles = css`
    :host {
      display: block;
      position: sticky;
      top: 0;
      z-index: 100;
      background: #16213e;
      border-bottom: 2px solid #0f3460;
    }
    .header {
      display: flex;
      justify-content: space-between;
      align-items: center;
      padding: 8px 24px;
      border-bottom: 1px solid #0f3460;
    }
    .header h1 { font-size: 1.4rem; font-weight: 600; color: #e0e0e0; margin: 0; }
    .header .meta { color: #8899aa; font-size: 0.85rem; }

    .commit-info {
      padding: 8px 24px;
      font-family: 'Consolas', 'Monaco', monospace;
      font-size: 0.85rem;
      color: #8899aa;
      border-bottom: 1px solid #0f3460;
      background: #131b30;
    }
    .commit-info .label { color: #e94560; }
    .commit-info .value { color: #e0e0e0; }

    .controls {
      padding: 10px 24px;
      display: flex;
      flex-wrap: wrap;
      gap: 12px;
      align-items: flex-start;
    }
    .radio-group {
      display: flex;
      flex-wrap: wrap;
      align-items: center;
      gap: 2px;
    }
    .group-label {
      font-size: 0.75rem;
      color: #8899aa;
      text-transform: uppercase;
      letter-spacing: 0.5px;
      margin-right: 6px;
      font-weight: 600;
    }
    label.radio {
      display: flex;
      align-items: center;
      gap: 3px;
      font-size: 0.82rem;
      color: #e0e0e0;
      cursor: pointer;
      padding: 3px 6px;
      border-radius: 4px;
      transition: background 0.15s;
    }
    label.radio:hover { background: rgba(233,69,96,0.12); }
    input[type="radio"] {
      accent-color: #e94560;
      margin: 0;
    }
    .dot {
      display: inline-block;
      width: 10px;
      height: 10px;
      border-radius: 50%;
    }

    .tabs {
      display: flex;
      gap: 0;
      padding: 0 24px;
      border-top: 1px solid #0f3460;
    }
    .tab-btn {
      padding: 10px 24px;
      background: none;
      border: none;
      color: #8899aa;
      cursor: pointer;
      font-size: 0.95rem;
      border-bottom: 3px solid transparent;
      transition: all 0.2s;
    }
    .tab-btn:hover { color: #e0e0e0; }
    .tab-btn.active {
      color: #e94560;
      border-bottom-color: #e94560;
      font-weight: 600;
    }
  `;

  _showScenario() { return this.activeTab === 'trend' || this.activeTab === 'ablation' || this.activeTab === 'budget'; }
  _showConfig()   { return this.activeTab === 'trend' || this.activeTab === 'matrix'; }
  _showPaths()    { return this.activeTab !== 'sweep'; }
  _showBackend()  { return this.activeTab === 'ablation'; }
  _showControls() { return this.activeTab !== 'sweep'; }

  render() {
    const ci = this.commitInfo;
    return html`
      <div class="header">
        <h1>Raptorpath Benchmark Dashboard</h1>
        <div class="meta">Generated: __GENERATED__</div>
      </div>

      <div class="tabs">
        ${['trend', 'matrix', 'sweep', 'ablation', 'budget'].map(t => html`
          <button class="tab-btn ${t === this.activeTab ? 'active' : ''}"
            @click=${() => this._fire('tab-change', t)}>
            ${{trend:'Trend', matrix:'Matrix', sweep:'Loss Sweep', ablation:'Ablation', budget:'Budget'}[t]}
          </button>
        `)}
      </div>

      ${this._showControls() ? html`
      <div class="controls">
        ${this._showScenario() ? html`
        <div class="radio-group">
          <span class="group-label">Scenario</span>
          ${ALL_SCENARIOS.map(s => html`
            <label class="radio">
              <input type="radio" name="scenario" value=${s}
                ?checked=${s === this.scenario}
                @change=${this._onScenario}>
              ${s}
            </label>
          `)}
        </div>` : ''}

        ${this._showConfig() ? html`
        <div class="radio-group">
          <span class="group-label">Config</span>
          ${ALL_CONFIGS.map(c => html`
            <label class="radio">
              <input type="radio" name="config" value=${c}
                ?checked=${c === this.config}
                @change=${this._onConfig}>
              ${c}
            </label>
          `)}
        </div>` : ''}

        ${this._showPaths() ? html`
        <div class="radio-group">
          <span class="group-label">Paths</span>
          ${ALL_PATHS.map(p => html`
            <label class="radio">
              <input type="radio" name="paths" value=${p}
                ?checked=${p === this.paths}
                @change=${this._onPaths}>
              ${p}
            </label>
          `)}
        </div>` : ''}

        ${this._showBackend() ? html`
        <div class="radio-group">
          <span class="group-label">Backend</span>
          ${ALL_BACKENDS.map(b => html`
            <label class="radio">
              <input type="radio" name="ablation-backend" value=${b}
                ?checked=${b === this.ablationBackend}
                @change=${this._onAblationBackend}>
              <span class="dot" style="background:${BACKEND_COLORS[b] || '#888'}"></span>
              ${b}
            </label>
          `)}
        </div>` : ''}
      </div>` : ''}

      <bench-timeline
        .selectedIdx=${this.selectedRunIdx}
        .visible=${this.activeTab !== 'trend'}>
      </bench-timeline>

      <div class="commit-info">
        ${ci
          ? html`<span class="label">Commit:</span> <span class="value">${ci.hash}</span> &nbsp;
                 <span class="label">Message:</span> <span class="value">${ci.message}</span> &nbsp;
                 <span class="label">Timestamp:</span> <span class="value">${ci.timestamp}</span>`
          : html`Click a chart data point to see commit details.`}
      </div>
    `;
  }

  _fire(name, value) {
    this.dispatchEvent(new CustomEvent(name, { detail: value, bubbles: true, composed: true }));
  }
  _onScenario(e) { this._fire('control-change', { scenario: e.target.value }); }
  _onConfig(e) { this._fire('control-change', { config: e.target.value }); }
  _onPaths(e) { this._fire('control-change', { paths: parseInt(e.target.value) }); }
  _onAblationBackend(e) { this._fire('control-change', { ablationBackend: e.target.value }); }
}
customElements.define('bench-controls', BenchControls);

// =====================================================
// <bench-dashboard> (root)
// =====================================================
class BenchDashboard extends LitElement {
  static properties = {
    scenario: { type: String },
    config: { type: String },
    paths: { type: Number },
    ablationBackend: { type: String },
    activeTab: { type: String },
    commitInfo: { type: Object },
    selectedRunIdx: { type: Number },
  };

  static styles = css`
    :host { display: block; }
    .content { padding: 20px 24px; }
    .chart-card {
      background: #16213e;
      border-radius: 8px;
      border: 1px solid #0f3460;
      padding: 8px;
      margin-bottom: 16px;
      min-height: 400px;
    }
    .chart-card .chart-div { width: 100%; height: 400px; position: relative; }
    .tab-content { display: none; }
    .tab-content.active { display: block; }
    /* Plotly modebar layout — normally injected into document.head by Plotly,
       but doesn't penetrate shadow DOM, so replicated here */
    .modebar-container {
      position: absolute;
      top: 0;
      right: 0;
      width: 100%;
    }
    .modebar {
      position: absolute;
      top: 2px;
      right: 2px;
      z-index: 10;
      display: flex;
    }
    .modebar-group {
      display: flex;
      padding-left: 8px;
      background: transparent !important;
    }
    .modebar-btn {
      position: relative;
      font-size: 16px;
      padding: 3px 4px;
      height: 22px;
      cursor: pointer;
    }
    .modebar-btn path { fill: #8899aa !important; }
    .modebar-btn:hover path { fill: #e94560 !important; }
    .modebar-btn.active path { fill: #e94560 !important; }
    /* Plotly SVG interaction — required for hover/click to work in shadow DOM */
    .js-plotly-plot .plotly .svg-container {
      position: relative;
      width: 100%;
      height: 100%;
      overflow: hidden;
    }
    .js-plotly-plot .plotly .main-svg {
      position: absolute;
      top: 0;
      left: 0;
      pointer-events: none;
    }
    .js-plotly-plot .plotly .main-svg .draglayer {
      pointer-events: all;
    }
    .js-plotly-plot .plotly .cursor-pointer {
      cursor: pointer;
    }
    .js-plotly-plot .plotly .cursor-crosshair {
      cursor: crosshair;
    }
  `;

  constructor() {
    super();
    this.scenario = ALL_SCENARIOS.includes('WiFi') ? 'WiFi' : ALL_SCENARIOS[0];
    this.config = ALL_CONFIGS.includes('baseline') ? 'baseline' : ALL_CONFIGS[0];
    this.paths = ALL_PATHS.includes(1) ? 1 : ALL_PATHS[0];
    this.ablationBackend = ALL_BACKENDS[0];
    this.activeTab = 'trend';
    this.commitInfo = null;
    this.selectedRunIdx = RUNS.length - 1;
    this._hiddenBackends = new Set();
    this._bindedDivs = new Set();
  }

  render() {
    return html`
      <bench-controls
        .scenario=${this.scenario}
        .config=${this.config}
        .paths=${this.paths}
        .ablationBackend=${this.ablationBackend}
        .activeTab=${this.activeTab}
        .commitInfo=${this.commitInfo}
        .selectedRunIdx=${this.selectedRunIdx}
        @control-change=${this._onControlChange}
        @tab-change=${this._onTabChange}
        @run-change=${this._onRunChange}>
      </bench-controls>

      <div class="content">
        <div class="tab-content ${this.activeTab === 'trend' ? 'active' : ''}">
          ${ALL_METRICS.map(m => html`
            <div class="chart-card">
              <div class="chart-div" id="chart-trend-${m}"></div>
            </div>
          `)}
        </div>

        <div class="tab-content ${this.activeTab === 'matrix' ? 'active' : ''}">
          ${ALL_METRICS.map(m => html`
            <div class="chart-card">
              <div class="chart-div" id="chart-matrix-${m}"></div>
            </div>
          `)}
        </div>

        <div class="tab-content ${this.activeTab === 'sweep' ? 'active' : ''}">
          <div class="chart-card">
            <div class="chart-div" id="chart-sweep-uniform"></div>
          </div>
          <div class="chart-card">
            <div class="chart-div" id="chart-sweep-bursty"></div>
          </div>
        </div>

        <div class="tab-content ${this.activeTab === 'ablation' ? 'active' : ''}">
          ${ALL_METRICS.map(m => html`
            <div class="chart-card">
              <div class="chart-div" id="chart-ablation-${m}"></div>
            </div>
          `)}
        </div>

        <div class="tab-content ${this.activeTab === 'budget' ? 'active' : ''}">
          <div class="chart-card">
            <div class="chart-div" id="chart-budget-waterfall"></div>
          </div>
          <div class="chart-card">
            <div class="chart-div" id="chart-budget-timeseries"></div>
          </div>
          <div class="chart-card">
            <div class="chart-div" id="chart-budget-gap"></div>
          </div>
        </div>
      </div>
    `;
  }

  _onControlChange(e) {
    const d = e.detail;
    if (d.scenario !== undefined) this.scenario = d.scenario;
    if (d.config !== undefined) this.config = d.config;
    if (d.paths !== undefined) this.paths = d.paths;
    if (d.ablationBackend !== undefined) this.ablationBackend = d.ablationBackend;
  }

  _onTabChange(e) {
    this.activeTab = e.detail;
  }

  _onRunChange(e) {
    this.selectedRunIdx = e.detail;
    this._showCommitInfo(RUNS[this.selectedRunIdx]);
  }

  _showCommitInfo(run) {
    this.commitInfo = {
      hash: run.commit_hash,
      message: run.commit_message,
      timestamp: fmtTimestampFull(run.timestamp),
    };
  }

  updated(changedProps) {
    if (changedProps.size === 1 && changedProps.has('commitInfo')) return;
    this.updateComplete.then(() => {
      requestAnimationFrame(() => this._renderCharts());
    });
  }

  _renderCharts() {
    const tab = this.activeTab;
    if (tab === 'trend') this._renderAllTrend();
    else if (tab === 'matrix') this._renderAllMatrix();
    else if (tab === 'sweep') this._renderAllSweep();
    else if (tab === 'ablation') this._renderAllAblation();
    else if (tab === 'budget') this._renderAllBudget();
  }

  _getChartDiv(id) {
    return this.renderRoot.getElementById(id);
  }

  // --- Legend sync helper ---
  _syncLegend(prefix, divIds, data) {
    const backend = data.data[data.curveNumber].name;
    const isHidden = this._hiddenBackends.has(backend);
    if (isHidden) this._hiddenBackends.delete(backend);
    else this._hiddenBackends.add(backend);
    const vis = isHidden ? true : 'legendonly';
    for (const id of divIds) {
      const d = this._getChartDiv(id);
      if (!d || !d.data) continue;
      const idx = d.data.findIndex(t => t.name === backend);
      if (idx >= 0) Plotly.restyle(d, { visible: vis }, [idx]);
    }
    return false; // prevent Plotly default
  }

  // --- TREND (one chart per metric) ---
  _renderAllTrend() {
    const { scenario, config, paths } = this;
    const xDates = RUNS.map(r => parseTimestamp(r.timestamp));
    const divIds = ALL_METRICS.map(m => `chart-trend-${m}`);

    for (const metric of ALL_METRICS) {
      const div = this._getChartDiv(`chart-trend-${metric}`);
      if (!div) continue;

      const traces = [];
      for (const backend of ALL_BACKENDS) {
        const ys = [], errs = [], customdata = [];
        for (let i = 0; i < RUNS.length; i++) {
          const run = RUNS[i];
          const entry = run.matrix.find(e =>
            e.backend === backend && e.scenario === scenario &&
            e.config === config && e.paths === paths
          );
          if (entry && entry.metrics[metric]) {
            ys.push(entry.metrics[metric].mean);
            errs.push(entry.metrics[metric].ci95);
          } else {
            ys.push(null);
            errs.push(0);
          }
          customdata.push(i);
        }
        traces.push({
          x: xDates, y: ys,
          error_y: { type: 'data', array: errs, visible: true, thickness: 1.5 },
          mode: 'lines+markers',
          name: backend,
          visible: this._hiddenBackends.has(backend) ? 'legendonly' : true,
          line: { color: BACKEND_COLORS[backend] || '#888' },
          marker: { size: 8 },
          customdata,
          hovertemplate: '%{x|%Y-%m-%d %H:%M}<br>%{y:.3f} \u00b1 %{error_y.array:.3f}<extra>' + backend + '</extra>'
        });
      }

      const layout = {
        ...PLOT_LAYOUT,
        title: { text: metric, font: { size: 14 } },
        yaxis: { ...PLOT_LAYOUT.yaxis, title: metric },
        xaxis: { ...PLOT_LAYOUT.xaxis, type: 'date', tickformat: '%b %d\n%H:%M' },
        showlegend: true,
        legend: { orientation: 'h', y: -0.2 },
        height: 380,
      };

      Plotly.react(div, traces, layout, PLOT_CONFIG);
      if (!this._bindedDivs.has(div)) {
        this._bindedDivs.add(div);
        div.on('plotly_click', (data) => {
          const idx = data.points[0].customdata;
          this._showCommitInfo(RUNS[idx]);
        });
        div.on('plotly_legendclick', (data) => this._syncLegend('trend', divIds, data));
      }
    }
  }

  // --- MATRIX (one heatmap per metric) ---
  _renderAllMatrix() {
    const { config, paths, selectedRunIdx } = this;
    const run = RUNS[selectedRunIdx];
    const hashLabel = run.commit_hash.substring(0, 7);

    for (const metric of ALL_METRICS) {
      const div = this._getChartDiv(`chart-matrix-${metric}`);
      if (!div) continue;

      const z = [], annotations = [];
      for (const backend of ALL_BACKENDS) {
        const row = [];
        for (const scenario of ALL_SCENARIOS) {
          const entry = run.matrix.find(e =>
            e.backend === backend && e.scenario === scenario &&
            e.config === config && e.paths === paths
          );
          let val = null;
          if (entry && entry.metrics[metric]) val = entry.metrics[metric].mean;
          row.push(val);
          annotations.push({
            x: scenario, y: backend,
            text: val !== null ? val.toFixed(2) : 'N/A',
            font: { color: '#fff', size: 11 },
            showarrow: false
          });
        }
        z.push(row);
      }

      const colorscale = REVERSED_METRICS.has(metric)
        ? [[0, '#2e7d32'], [0.5, '#f9a825'], [1, '#c62828']]
        : [[0, '#c62828'], [0.5, '#f9a825'], [1, '#2e7d32']];

      const trace = {
        z, x: ALL_SCENARIOS, y: ALL_BACKENDS,
        type: 'heatmap', colorscale,
        hovertemplate: '%{y} / %{x}: %{z:.4f}<extra></extra>'
      };

      const layout = {
        ...PLOT_LAYOUT,
        title: { text: metric + ' \u2014 ' + config + ' / ' + paths + 'p (' + hashLabel + ')', font: { size: 14 } },
        annotations,
        xaxis: { ...PLOT_LAYOUT.xaxis, title: '', side: 'bottom' },
        yaxis: { ...PLOT_LAYOUT.yaxis, title: '', autorange: 'reversed' },
        margin: { ...PLOT_LAYOUT.margin, l: 110 },
        height: 380,
      };

      Plotly.react(div, [trace], layout, PLOT_CONFIG);
      if (!this._bindedDivs.has(div)) {
        this._bindedDivs.add(div);
        div.on('plotly_click', () => this._showCommitInfo(RUNS[this.selectedRunIdx]));
      }
    }
  }

  // --- SWEEP (both uniform + bursty) ---
  _renderAllSweep() {
    const run = RUNS[this.selectedRunIdx];
    const models = [
      { key: 'table1_uniform', divId: 'chart-sweep-uniform', label: 'Recovery vs Loss \u2014 Uniform' },
      { key: 'table1b_bursty', divId: 'chart-sweep-bursty', label: 'Recovery vs Loss \u2014 Bursty (Gilbert-Elliott)' },
    ];
    const divIds = models.map(m => m.divId);

    for (const { key, divId, label } of models) {
      const div = this._getChartDiv(divId);
      if (!div) continue;
      const data = run[key];
      if (!data) continue;

      const backends = [...new Set(data.map(e => e.backend))];
      const traces = backends.map(backend => {
        const entries = data.filter(e => e.backend === backend).sort((a,b) => a.loss_pct - b.loss_pct);
        return {
          x: entries.map(e => e.loss_pct),
          y: entries.map(e => e.recovery.mean),
          error_y: { type: 'data', array: entries.map(e => e.recovery.ci95), visible: true, thickness: 1.5 },
          mode: 'lines+markers',
          name: backend,
          visible: this._hiddenBackends.has(backend) ? 'legendonly' : true,
          line: { color: BACKEND_COLORS[backend] || '#888' },
          marker: { size: 7 },
          hovertemplate: 'Loss %{x}%: %{y:.2f}% \u00b1 %{error_y.array:.3f}<extra>' + backend + '</extra>'
        };
      });

      const layout = {
        ...PLOT_LAYOUT,
        title: { text: label, font: { size: 14 } },
        xaxis: { ...PLOT_LAYOUT.xaxis, title: 'Loss Rate (%)', dtick: 5 },
        yaxis: { ...PLOT_LAYOUT.yaxis, title: 'Recovery Rate (%)', range: [0, 105] },
        showlegend: true,
        legend: { orientation: 'h', y: -0.2 },
        height: 380,
      };

      Plotly.react(div, traces, layout, PLOT_CONFIG);
      if (!this._bindedDivs.has(div)) {
        this._bindedDivs.add(div);
        div.on('plotly_click', () => this._showCommitInfo(RUNS[this.selectedRunIdx]));
        div.on('plotly_legendclick', (data) => this._syncLegend('sweep', divIds, data));
      }
    }
  }

  // --- ABLATION (one bar chart per metric) ---
  _renderAllAblation() {
    const { scenario, paths, ablationBackend, selectedRunIdx } = this;
    const run = RUNS[selectedRunIdx];
    const configs = ['baseline', 'no_nack', 'no_reorder', 'no_pi'];
    const configLabels = { baseline: 'Baseline', no_nack: 'No NACK', no_reorder: 'No Reorder', no_pi: 'No PI' };
    const configColors = ['#2196F3', '#FF9800', '#E91E63', '#9C27B0'];
    const backend = ablationBackend;

    for (const metric of ALL_METRICS) {
      const div = this._getChartDiv(`chart-ablation-${metric}`);
      if (!div) continue;

      const vals = [], errs = [], colors = [];
      for (let i = 0; i < configs.length; i++) {
        const cfg = configs[i];
        const entry = run.matrix.find(e =>
          e.backend === backend && e.scenario === scenario &&
          e.config === cfg && e.paths === paths
        );
        if (entry && entry.metrics[metric]) {
          vals.push(entry.metrics[metric].mean);
          errs.push(entry.metrics[metric].ci95);
        } else {
          vals.push(0);
          errs.push(0);
        }
        colors.push(configColors[i]);
      }

      const trace = {
        x: configs.map(c => configLabels[c] || c),
        y: vals,
        error_y: { type: 'data', array: errs, visible: true, thickness: 1.5 },
        type: 'bar',
        marker: { color: colors },
        hovertemplate: '%{x}: %{y:.4f} \u00b1 %{error_y.array:.4f}<extra></extra>'
      };

      const layout = {
        ...PLOT_LAYOUT,
        title: { text: metric + ' \u2014 ' + backend + ' / ' + scenario + ' / ' + paths + 'p', font: { size: 14 } },
        yaxis: { ...PLOT_LAYOUT.yaxis, title: metric },
        showlegend: false,
        bargap: 0.3,
        height: 380,
      };

      Plotly.react(div, [trace], layout, PLOT_CONFIG);
      if (!this._bindedDivs.has(div)) {
        this._bindedDivs.add(div);
        div.on('plotly_click', () => this._showCommitInfo(RUNS[this.selectedRunIdx]));
      }
    }
  }
  // --- BUDGET TAB (ADR-0050: FEC budget visualization) ---
  _renderAllBudget() {
    const { scenario, paths, selectedRunIdx } = this;
    const run = RUNS[selectedRunIdx];

    // Budget waterfall: break down overhead into proactive | nack | wasted | spare
    const waterfallDiv = this._getChartDiv('chart-budget-waterfall');
    if (waterfallDiv) {
      const backends = ALL_BACKENDS.filter(b => b !== 'Retransmit');
      const overhead = [], theoretical = [];
      for (const backend of backends) {
        const entry = run.matrix.find(e =>
          e.backend === backend && e.scenario === scenario &&
          e.config === 'baseline' && e.paths === paths
        );
        if (entry && entry.metrics.overhead_pct) {
          overhead.push(entry.metrics.overhead_pct.mean);
          // Information-theoretic minimum: p/(1-p) where p = loss rate
          const loss = entry.metrics.loss_rate ? entry.metrics.loss_rate.mean : 0;
          theoretical.push(loss > 0 ? (loss / (1 - loss)) * 100 : 0);
        } else {
          overhead.push(0);
          theoretical.push(0);
        }
      }

      const traces = [
        {
          x: backends, y: theoretical,
          type: 'bar', name: 'IT Minimum',
          marker: { color: '#4CAF50' },
          hovertemplate: '%{x}: %{y:.2f}%<extra>IT Minimum</extra>'
        },
        {
          x: backends, y: overhead.map((o, i) => Math.max(0, o - theoretical[i])),
          type: 'bar', name: 'Estimation Tax',
          marker: { color: '#FF9800' },
          hovertemplate: '%{x}: %{y:.2f}%<extra>Estimation Tax</extra>'
        }
      ];

      const layout = {
        ...PLOT_LAYOUT,
        title: { text: 'Budget Waterfall \u2014 ' + scenario + ' / ' + paths + 'p', font: { size: 14 } },
        barmode: 'stack',
        yaxis: { ...PLOT_LAYOUT.yaxis, title: 'Overhead (%)' },
        showlegend: true,
        legend: { orientation: 'h', y: -0.2 },
        height: 380,
      };
      Plotly.react(waterfallDiv, traces, layout, PLOT_CONFIG);
    }

    // Time-series: overhead trend across runs
    const tsDiv = this._getChartDiv('chart-budget-timeseries');
    if (tsDiv) {
      const backends = ALL_BACKENDS.filter(b => b !== 'Retransmit');
      const traces = backends.map(backend => {
        const xs = [], ys = [];
        for (let i = 0; i < RUNS.length; i++) {
          const entry = RUNS[i].matrix.find(e =>
            e.backend === backend && e.scenario === scenario &&
            e.config === 'baseline' && e.paths === paths
          );
          if (entry && entry.metrics.overhead_pct) {
            xs.push(parseTimestamp(RUNS[i].timestamp));
            ys.push(entry.metrics.overhead_pct.mean);
          }
        }
        return {
          x: xs, y: ys,
          mode: 'lines+markers', name: backend,
          line: { color: BACKEND_COLORS[backend] || '#888' },
          marker: { size: 6 },
          hovertemplate: '%{x|%b %d}: %{y:.2f}%<extra>' + backend + '</extra>'
        };
      }).filter(t => t.x.length > 0);

      const layout = {
        ...PLOT_LAYOUT,
        title: { text: 'Overhead Trend \u2014 ' + scenario + ' / ' + paths + 'p', font: { size: 14 } },
        xaxis: { ...PLOT_LAYOUT.xaxis, type: 'date', title: 'Date' },
        yaxis: { ...PLOT_LAYOUT.yaxis, title: 'Overhead (%)' },
        showlegend: true,
        legend: { orientation: 'h', y: -0.25 },
        height: 380,
      };
      Plotly.react(tsDiv, traces, layout, PLOT_CONFIG);
    }

    // Estimation gap: actual overhead / IT minimum ratio
    const gapDiv = this._getChartDiv('chart-budget-gap');
    if (gapDiv) {
      const scenarios = ALL_SCENARIOS;
      const backends = ALL_BACKENDS.filter(b => b !== 'Retransmit');
      const traces = backends.map(backend => {
        const ratios = scenarios.map(sc => {
          const entry = run.matrix.find(e =>
            e.backend === backend && e.scenario === sc &&
            e.config === 'baseline' && e.paths === paths
          );
          if (!entry || !entry.metrics.overhead_pct) return 0;
          const overhead = entry.metrics.overhead_pct.mean;
          const loss = entry.metrics.loss_rate ? entry.metrics.loss_rate.mean : 0;
          const it_min = loss > 0 ? (loss / (1 - loss)) * 100 : 0.01;
          return it_min > 0.01 ? overhead / it_min : 0;
        });
        return {
          x: scenarios, y: ratios,
          type: 'bar', name: backend,
          marker: { color: BACKEND_COLORS[backend] || '#888' },
          hovertemplate: '%{x}: %{y:.1f}x<extra>' + backend + '</extra>'
        };
      });

      const layout = {
        ...PLOT_LAYOUT,
        title: { text: 'Estimation Gap (Actual / IT Minimum) \u2014 ' + paths + 'p', font: { size: 14 } },
        barmode: 'group',
        yaxis: { ...PLOT_LAYOUT.yaxis, title: 'Gap Ratio (lower = better)', type: 'log' },
        showlegend: true,
        legend: { orientation: 'h', y: -0.25 },
        height: 380,
      };
      Plotly.react(gapDiv, traces, layout, PLOT_CONFIG);
    }
  }
}
customElements.define('bench-dashboard', BenchDashboard);
</script>
</body>
</html>"""


def main():
    runs = load_runs()
    now = datetime.now().strftime("%Y-%m-%d %H:%M:%S")
    html = HTML_TEMPLATE.replace("__DATA__", json.dumps(runs))
    html = html.replace("__GENERATED__", now)
    out_path = os.path.join(DOCS_DIR, "benchmark-dashboard.html")
    with open(out_path, "w", encoding="utf-8") as f:
        f.write(html)
    print(f"Dashboard written to {out_path}")


if __name__ == "__main__":
    main()
