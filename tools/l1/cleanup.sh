#!/bin/bash
# Idempotent cleanup: removes ONLY rp-* namespaces (and their veths, which
# die with the namespaces). Never touches root-namespace devices.
set -euo pipefail
for ns in $(ip netns list | awk '{print $1}' | grep '^rp-' || true); do
    echo "deleting $ns"
    sudo ip netns del "$ns" || true
done
echo "cleanup done"
