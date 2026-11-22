#!/bin/bash
# The backlog sampler of the disorder-price (omega) harness: a separate process (started and killed by PID by omega_battery.sh), so it inherits
# no traps, locks or shell state from the driver. Every OMEGA_SAMPLE_S (default 10 ms) it appends "<epoch> <backlog packets>" of cli0's qdisc
# (the bottleneck, inside rp-cli) to the file. Registration 4.4. All namespace access goes through nsx (omega_topo.sh), so the rp-* guard applies.
#   bash omega_sampler.sh <outfile>
HERE="$(cd "$(dirname "$0")" && pwd)"
source "$HERE/omega_topo.sh"
out="${1:?outfile}"
while :; do
  t="$(date +%s.%N)"
  raw="$(nsx "$OMEGA_NS_CLI" tc -s qdisc show dev cli0 2>/dev/null)"
  b="$(printf '%s
' "$raw" | awk '/backlog/ && !d {sub(/p$/,"",$3); print $3; d=1}')"
  [ -z "$b" ] || echo "$t $b" >> "$out"
  "${OMEGA_SLEEP:-sleep}" "${OMEGA_SAMPLE_S:-0.01}"
done
