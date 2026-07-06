#!/bin/bash
# Re-fetch the full original mahimahi cellular capacity traces used by
# real_trace_validation.rs. The repo vendors time-truncated copies (see
# PROVENANCE.md); this script downloads the verbatim originals.
set -euo pipefail
BASE="https://raw.githubusercontent.com/ravinet/mahimahi/master/traces"
DEST="$(cd "$(dirname "$0")" && pwd)"
for f in Verizon-LTE-short.down ATT-LTE-driving-2016.down \
         TMobile-UMTS-driving.down TMobile-LTE-short.down \
         Verizon-LTE-driving.down; do
    curl -sSL "$BASE/$f" -o "$DEST/$f.full"
    echo "fetched $f.full ($(wc -l < "$DEST/$f.full") lines)"
done
echo "Done. Truncate to first 120 s to reproduce the vendored copies."
