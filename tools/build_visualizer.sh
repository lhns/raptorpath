#!/bin/bash
# Build the interactive visualizer with embedded WASM.
# Produces a single self-contained HTML file that works from file://.
#
# Usage: bash tools/build_visualizer.sh

set -euo pipefail
cd "$(dirname "$0")/.."

echo "Building WASM..."
wasm-pack build raptorpath-wasm --target web --out-dir ../raptorpath/docs/wasm 2>&1 | tail -3

echo "Base64-encoding WASM binary..."
WASM_B64=$(base64 -w0 raptorpath/docs/wasm/raptorpath_wasm_bg.wasm)
echo "  WASM size: $(wc -c < raptorpath/docs/wasm/raptorpath_wasm_bg.wasm) bytes"
echo "  Base64 size: ${#WASM_B64} chars"

echo "Generating inline visualizer..."

TMPFILE=$(mktemp)
cat > "$TMPFILE" <<PYEOF
import re

html = open('raptorpath/docs/interactive-visualizer.html').read()

# Read glue JS, strip export keywords but keep function names
glue = open('raptorpath/docs/wasm/raptorpath_wasm.js').read()
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

// --- Simulation wrapper ---
const NUM_SOURCE = 200;
const SMOOTH_WINDOW = 6;

class SimWrapper {{
  constructor(params) {{
    const rho = params.rho || 1.0;
    this.inner = new Simulation(params.eps, params.q, params.rttMs, params.W, params.r, rho);
    this.rateHistory = [];
    this.channelStates = [];
    this.lossEvents = [];
    this.eps = params.eps;
    this.q = params.q;
    const pp = params.eps * params.q / (1 - params.eps);
    this.sigma2 = burst_variance_factor(pp, params.q);
  }}
  get finished() {{ return this.inner.is_finished(); }}
  get rStar() {{ return this.inner.get_r_star(); }}
  get rStarAuto() {{ return this.inner.get_r_star_auto(); }}
  get totalSrc() {{ return this.inner.get_total_src(); }}
  get totalFec() {{ return this.inner.get_total_fec(); }}
  get totalArq() {{ return this.inner.get_total_arq(); }}
  get totalLost() {{ return this.inner.get_total_lost(); }}
  get cumDecoded() {{ return this.inner.get_cum_decoded(); }}
  get tick() {{ return this.inner.get_tick(); }}
  retxBufSize() {{ return this.inner.get_retx_buf_size(); }}
  step() {{
    if (this.finished) return;
    this.inner.step();
    this.channelStates.push(this.inner.channel_is_good() ? "good" : "bad");
    const lost = this.inner.get_lost();
    if (lost > 0) this.lossEvents.push(this.inner.get_tick() - 1);
    const pct = 100 / this.inner.get_num_source();
    this.rateHistory.push({{
      src: this.inner.get_src(), fec: this.inner.get_fec(),
      arq: this.inner.get_arq(), lost,
      cumSent: this.inner.get_cum_sent() * pct,
      cumArrived: this.inner.get_cum_arrived() * pct,
      cumDecoded: this.inner.get_cum_decoded() * pct,
    }});
  }}
  smoothedRate(idx) {{
    const w = SMOOTH_WINDOW;
    const start = Math.max(0, idx - w + 1), end = idx + 1;
    let s=0,f=0,a=0,l=0,n=0;
    for (let i=start; i<end && i<this.rateHistory.length; i++) {{
      s+=this.rateHistory[i].src; f+=this.rateHistory[i].fec;
      a+=this.rateHistory[i].arq; l+=this.rateHistory[i].lost; n++;
    }}
    const h = this.rateHistory[idx];
    return n>0 ? {{src:s/n, fec:f/n, arq:a/n, lost:l/n,
                  cumSent:h.cumSent, cumArrived:h.cumArrived, cumDecoded:h.cumDecoded}}
               : {{src:0,fec:0,arq:0,lost:0,cumSent:0,cumArrived:0,cumDecoded:0}};
  }}
}}

'''

# Change module script to regular script
html = html.replace('<script type="module">', '<script>')

# Replace everything from "WASM MODULE" to just before "UI" section
pattern = r'// =+\n// WASM MODULE.*?(?=// =+\n// UI)'
html = re.sub(pattern, init_block, html, flags=re.DOTALL)

# In the UI section, replace 'wasm.xxx(' and 'wasm_mod.xxx(' with direct function calls.
# The glue functions are now global, so no prefix needed.
for fn in ['burst_variance_factor', 'compute_r_star', 'taper_density',
           'find_t_cut', 'solve_r_from_delta_rho', 'solve_delta_from_r_rho',
           'solve_rho_from_r_delta']:
    html = html.replace(f'wasm_mod.{fn}(', f'{fn}(')
    html = html.replace(f'wasm.{fn}(', f'{fn}(')

# Fix Simulation constructor references
html = html.replace('new wasm_mod.Simulation(', 'new Simulation(')
html = html.replace('new wasm.Simulation(', 'new Simulation(')

# Fix overhead/recovery metric calls
html = html.replace('sim.inner ? sim.inner.get_overhead() : 0', 'sim.inner.get_overhead()')
html = html.replace('sim.inner ? sim.inner.get_recovery() : 100', 'sim.inner.get_recovery()')

open('raptorpath/docs/interactive-visualizer.html', 'w').write(html)
print(f"HTML size: {len(html)} bytes")
PYEOF

python3 "$TMPFILE"
rm -f "$TMPFILE"

echo "Done! Visualizer updated: raptorpath/docs/interactive-visualizer.html"
echo "Open it directly in a browser (file:// works)."
