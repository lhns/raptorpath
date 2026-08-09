#!/bin/bash
# Transport-ceiling diagnosis: single-path (or dual) systematic-repair perf
# transfer with RWM_DIAG=1, capturing the per-250ms constraint report from
# BOTH server and client so the sender side (wherever run_window_sender runs)
# is observed.
#
#   sudo bash diag_rwm.sh <scenA> <scenB> <bytes> <mode:single|dual>
set -uo pipefail
cd "$(dirname "$0")"
source ./lib.sh
BIN="/home/vibe/raptorpath/target/release/raptorpath"
SCENA="${1:-c2}"; SCENB="${2:-c2}"; BYTES="${3:-50000000}"; MODE="${4:-single}"

TENV="$(rwm_forward_env) RWM_DIAG=1"   # gate forwarding: ONE shared list in lib.sh
EXTRA="${RWM_EXTRA:---window-systematic-repair}"

cleanup() { pkill -x raptorpath 2>/dev/null || true; bash ./topo_dual.sh down >/dev/null 2>&1 || true; }
trap cleanup EXIT
cleanup
if pgrep -x raptorpath >/dev/null 2>&1; then echo "BUSY -- abort" >&2; exit 3; fi

bash ./topo_dual.sh up "$SCENA" "$SCENB" --seed 42 >/dev/null 2>&1

if [[ "$MODE" == "dual" ]]; then
    SRV_BIND="10.77.0.2:7000,10.78.0.2:7000"; PEERS="10.77.0.2:7000,10.78.0.2:7000"; CLI_BIND="10.77.0.1:0,10.78.0.1:0"
else
    SRV_BIND="10.77.0.2:7000"; PEERS="10.77.0.2:7000"; CLI_BIND="10.77.0.1:0"
fi

ip netns exec "$NS_SRV" env $TENV "$BIN" perf --server --bind "$SRV_BIND" \
    --window-reliable $EXTRA --protocol-hint bulk >/tmp/diag-s.log 2>&1 &
for _ in $(seq 1 20); do ip netns exec "$NS_SRV" ss -uln 2>/dev/null | grep -q ':7000' && break; sleep 0.3; done
sleep 1

echo "--- DIAG mode=$MODE A=$SCENA B=$SCENB extra='$EXTRA' env='$TENV' ($BYTES) start=$(date +%T)"
timeout 200 ip netns exec "$NS_CLI" env $TENV "$BIN" perf --client \
    --peer "$PEERS" --bind "$CLI_BIND" --window-reliable $EXTRA --protocol-hint bulk \
    --bytes "$BYTES" --runs 1 >/tmp/diag-c.log 2>&1 \
    || echo "CLIENT DNF/timeout"
echo "    done $(date +%T)"

echo "=== summary line(s) ==="
grep -hE "summary|dnf" /tmp/diag-c.log | tail -3
echo "=== [DIAG] from SERVER (sender side if server transmits) ==="
sed 's/\x1b\[[0-9;]*m//g' /tmp/diag-s.log | grep '\[DIAG\]' | tail -40
echo "=== [DIAG] from CLIENT ==="
sed 's/\x1b\[[0-9;]*m//g' /tmp/diag-c.log | grep '\[DIAG\]' | tail -40
