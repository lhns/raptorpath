#!/bin/bash
# Build the interactive visualizer with embedded WASM.
# Produces a single self-contained HTML file that works from file://.
#
# Usage: bash raptorpath-visualizer/build_visualizer.sh

set -euo pipefail
cd "$(dirname "$0")/.."

echo "Building WASM..."
wasm-pack build raptorpath-wasm --target web --out-dir ../raptorpath-visualizer/wasm 2>&1 | tail -3

echo "Base64-encoding WASM binary..."
WASM_B64=$(base64 -w0 raptorpath-visualizer/wasm/raptorpath_wasm_bg.wasm)
echo "  WASM size: $(wc -c < raptorpath-visualizer/wasm/raptorpath_wasm_bg.wasm) bytes"
echo "  Base64 size: ${#WASM_B64} chars"

echo "Generating inline visualizer..."

TMPFILE=$(mktemp)
cat > "$TMPFILE" <<PYEOF
import re

html = open('raptorpath-visualizer/interactive-visualizer.html', encoding='utf-8').read()

# Read glue JS, strip export keywords but keep function names
glue = open('raptorpath-visualizer/wasm/raptorpath_wasm.js', encoding='utf-8').read()
glue = glue.replace('export class ', 'class ')
glue = glue.replace('export function ', 'function ')
glue = re.sub(r'^export \{.*$', '', glue, flags=re.MULTILINE)
glue = re.sub(r'^/\* @ts-self-types.*$', '', glue, flags=re.MULTILINE)
# Remove async functions that use import.meta / multi-line strings
glue = re.sub(r'async function __wbg_load\(.*?^}', '', glue, flags=re.DOTALL|re.MULTILINE)
glue = re.sub(r'async function __wbg_init\(.*?^}', '', glue, flags=re.DOTALL|re.MULTILINE)

wasm_b64 = "${WASM_B64}"

# The init block includes: base64 wasm, glue code, initSync call, and SimWrapper.
# The glue functions (burst_variance_factor, etc.) call the raw 'wasm' object internally.
# The UI code calls these glue functions directly (they are global after inlining).
init_block = f'''// =========================================================================
// WASM MODULE - real Rust math compiled from raptorpath-math (embedded)
// =========================================================================
const WASM_B64 = "{wasm_b64}";
const wasmBytes = Uint8Array.from(atob(WASM_B64), c => c.charCodeAt(0));

// --- Inlined wasm-bindgen glue (functions call raw 'wasm' object internally) ---
{glue}
// --- End inlined glue ---

initSync({{ module: wasmBytes }});
''' + '''
// CamelCase wrappers for three-var solvers (UI uses camelCase, glue uses snake_case)
function solveRFromDeltaRho(eps,q,W,s2,delta,rho) {
  const a = solve_r_from_delta_rho(eps,q,W,s2,delta,rho);
  return {r:a[0], delta:a[1], rho:a[2], tCut:a[3]};
}
function solveDeltaFromRRho(eps,q,W,s2,r,rho) {
  const a = solve_delta_from_r_rho(eps,q,W,s2,r,rho);
  return {r:a[0], delta:a[1], rho:a[2], tCut:a[3]};
}
function solveRhoFromRDelta(eps,q,W,s2,r,delta) {
  const a = solve_rho_from_r_delta(eps,q,W,s2,r,delta);
  return {r:a[0], delta:a[1], rho:a[2], tCut:a[3]};
}

// --- Simulation wrapper ---
// The inner Simulation runs the SAME rate-controller code as the production
// transport (shared raptorpath-math::controller_rate), fed by an honest
// RTT-delayed estimator (BOCD p-hat, GE sigma2-hat).
const SMOOTH_WINDOW = 6;

class SimWrapper {
  constructor(params) {
    const fixedR = params.hint === 'fixed' ? params.fixedR : undefined;
    this.inner = new Simulation(params.eps, params.q, params.rttMs, params.W, params.hint,
                                fixedR, params.customDelta, params.customRho);
    this.rateHistory = [];
    this.channelStates = [];
    this.eps = params.eps;
    this.q = params.q;
    this.hint = params.hint;
    const pp = params.eps * params.q / (1 - params.eps);
    this.sigma2True = burst_variance_factor(pp, params.q);
    this.srcDoneTick = null;  // tick of the last source symbol (tail FEC starts here)
  }
  get finished() { return this.inner.is_finished(); }
  // live controller output vs closed-form reference at TRUE channel params
  get rLive()    { return this.inner.get_r_star(); }
  get rStarRef() { return this.inner.get_r_star_auto(); }
  // what the controller sees (estimator state)
  get pUpper()    { return this.inner.get_p_upper(); }
  get estLoss()   { return this.inner.get_estimated_loss(); }
  get sigma2Est() { return this.inner.get_sigma2_est(); }
  get deltaEff()  { return this.inner.get_delta_eff(); }
  get rSat()      { return this.inner.get_r_sat(); }
  // derived window W* (paper 8.8) from the LIVE estimator state
  get derivedW()  { return this.inner.get_derived_w(); }
  // custom triangle mode (rho < 1 enables T_cut give-up eviction)
  get givenUp()     { return this.inner.get_given_up(); }
  get reliability() { return this.inner.get_reliability(); }
  get rho()         { return this.inner.get_rho(); }
  // overhead decomposition: floor is the channel's fault, excess is the protocol's.
  // nominal floor = eps/(1-eps); realized floor = lost/arrived over THIS run's
  // actual sends (a finite run samples only a prefix of the calibrated channel).
  get overheadFloor()         { return this.inner.get_overhead_floor(); }
  get overheadFloorRealized() { return this.inner.get_overhead_floor_realized(); }
  get excessOverhead()        { return this.inner.get_excess_overhead(); }
  get totalSrc() { return this.inner.get_total_src(); }
  get totalFec() { return this.inner.get_total_fec(); }
  get totalArq() { return this.inner.get_total_arq(); }
  get totalLost() { return this.inner.get_total_lost(); }
  get cumDecoded() { return this.inner.get_cum_decoded(); }
  get tick() { return this.inner.get_tick(); }
  // delivery latency (ms): send -> decode + one-way propagation; given-up excluded
  get latLast() { return this.inner.get_lat_last(); }
  get latAvg()  { return this.inner.get_lat_avg(); }
  get latP50()  { return this.inner.get_lat_percentile(0.5); }
  get latP99()  { return this.inner.get_lat_percentile(0.99); }
  get jitter()  { return this.inner.get_jitter(); }
  retxBufSize() { return this.inner.get_retx_buf_size(); }
  step() {
    if (this.finished) return;
    this.inner.step();
    this.channelStates.push(this.inner.channel_is_good() ? "good" : "bad");
    const lost = this.inner.get_lost();
    const pct = 100 / this.inner.get_num_source();
    this.rateHistory.push({
      src: this.inner.get_src(), fec: this.inner.get_fec(),
      arq: this.inner.get_arq(), lost,
      cumSent: this.inner.get_cum_sent() * pct,
      cumArrived: this.inner.get_cum_arrived() * pct,
      cumDecoded: this.inner.get_cum_decoded() * pct,
    });
    if (this.srcDoneTick === null && this.inner.get_total_src() >= this.inner.get_num_source()) {
      this.srcDoneTick = this.rateHistory.length - 1;
    }
  }
  smoothedRate(idx) {
    const w = SMOOTH_WINDOW;
    const start = Math.max(0, idx - w + 1), end = idx + 1;
    let s=0,f=0,a=0,l=0,n=0;
    for (let i=start; i<end && i<this.rateHistory.length; i++) {
      s+=this.rateHistory[i].src; f+=this.rateHistory[i].fec;
      a+=this.rateHistory[i].arq; l+=this.rateHistory[i].lost; n++;
    }
    const h = this.rateHistory[idx];
    return n>0 ? {src:s/n, fec:f/n, arq:a/n, lost:l/n,
                  cumSent:h.cumSent, cumArrived:h.cumArrived, cumDecoded:h.cumDecoded}
               : {src:0,fec:0,arq:0,lost:0,cumSent:0,cumArrived:0,cumDecoded:0};
  }
}

'''

# Change module script to regular script
html = html.replace('<script type="module">', '<script>')

# Replace everything from "WASM MODULE" to just before "UI" section
pattern = r'// =+\n// WASM MODULE.*?(?=// =+\n// UI)'
html = re.sub(pattern, init_block, html, flags=re.DOTALL)

# No wasm.xxx -> xxx replacement needed: the source HTML already uses direct calls.
# The glue code's internal wasm.xxx references must NOT be touched.

open('raptorpath-visualizer/interactive-visualizer.html', 'w', encoding='utf-8', newline='\n').write(html)
print(f"HTML size: {len(html)} bytes")
PYEOF

python3 "$TMPFILE"
rm -f "$TMPFILE"

echo "Running engine tests against the built file..."
node raptorpath-visualizer/test_visualizer.mjs
echo "Engine tests passed."

echo "Done! Visualizer updated: raptorpath-visualizer/interactive-visualizer.html"
echo "Open it directly in a browser (file:// works)."
