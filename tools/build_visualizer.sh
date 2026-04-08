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

# Write the inline wasm init block to a temp file
TMPFILE=$(mktemp)
cat > "$TMPFILE" <<PYEOF
import re

html = open('raptorpath/docs/interactive-visualizer.html').read()

# Read glue JS, strip exports
glue = open('raptorpath/docs/wasm/raptorpath_wasm.js').read()
glue = glue.replace('export class ', 'class ')
glue = glue.replace('export function ', 'function ')
glue = re.sub(r'^export \{.*$', '', glue, flags=re.MULTILINE)
glue = re.sub(r'^/\* @ts-self-types.*$', '', glue, flags=re.MULTILINE)

wasm_b64 = "${WASM_B64}"

init_block = f'''// =========================================================================
// WASM MODULE — real Rust math compiled from raptorpath-math (embedded)
// =========================================================================
const WASM_B64 = "{wasm_b64}";
const wasmBytes = Uint8Array.from(atob(WASM_B64), c => c.charCodeAt(0));

// --- Inlined wasm-bindgen glue ---
{glue}
// --- End inlined glue ---

initSync(wasmBytes);

const wasm_mod = {{ burst_variance_factor, compute_r_star, taper_density,
  compute_delta, find_t_cut, Simulation,
  solve_r_from_delta_rho, solve_delta_from_r_rho, solve_rho_from_r_delta }};

'''

# Change module script to regular script
html = html.replace('<script type="module">', '<script>')

# Replace the import block
pattern = r'// =+\n// WASM MODULE.*?(?=// =+\n// UI)'
html = re.sub(pattern, init_block, html, flags=re.DOTALL)

# Replace wasm. calls with wasm_mod.
for fn in ['burst_variance_factor', 'compute_r_star', 'taper_density',
           'find_t_cut', 'Simulation',
           'solve_r_from_delta_rho', 'solve_delta_from_r_rho', 'solve_rho_from_r_delta']:
    html = html.replace(f'wasm.{fn}(', f'wasm_mod.{fn}(')
    html = html.replace(f'new wasm.{fn}(', f'new wasm_mod.{fn}(')

open('raptorpath/docs/interactive-visualizer.html', 'w').write(html)
print(f"HTML size: {len(html)} bytes")
PYEOF

python3 "$TMPFILE"
rm -f "$TMPFILE"

echo "Done! Visualizer updated: raptorpath/docs/interactive-visualizer.html"
echo "Open it directly in a browser (file:// works)."
