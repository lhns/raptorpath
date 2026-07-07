#!/bin/bash
# Profile a single-path C2 native-perf transfer: measure CPU utilisation of
# sender+receiver processes (CPU-bound vs latency-bound) and record a perf
# profile of the SENDER (client) hot path.
#
#   sudo bash prof_single.sh <G> <bytes> <seconds_to_profile>
set -uo pipefail
cd "$(dirname "$0")"
source ./lib.sh
BIN="/home/vibe/raptorpath/target/release/raptorpath"
G="${1:-384}"; BYTES="${2:-50000000}"; PROF_S="${3:-20}"
export RWM_GEN="$G" RWM_GEN_R=0.15

cleanup() {
    pkill -x raptorpath 2>/dev/null || true
    bash ./topo_dual.sh down >/dev/null 2>&1 || true
}
trap cleanup EXIT
cleanup
pgrep -x raptorpath >/dev/null 2>&1 && { echo "BUSY"; exit 3; }

bash ./topo_dual.sh up c2 c2 --seed 42 >/dev/null 2>&1

SRV_BIND="10.77.0.2:7000"; PEERS="10.77.0.2:7000"; CLI_BIND="10.77.0.1:0"
EXTRA="--window-systematic-repair"

ip netns exec "$NS_SRV" env RWM_GEN=$G RWM_GEN_R=0.15 "$BIN" perf --server \
    --bind "$SRV_BIND" --window-reliable $EXTRA --protocol-hint bulk \
    >/tmp/prof-s.log 2>&1 &
for _ in $(seq 1 20); do
    ip netns exec "$NS_SRV" ss -uln 2>/dev/null | grep -q ':7000' && break
    sleep 0.3
done
sleep 1

ip netns exec "$NS_CLI" env RWM_GEN=$G RWM_GEN_R=0.15 "$BIN" perf --client \
    --peer "$PEERS" --bind "$CLI_BIND" --window-reliable $EXTRA \
    --protocol-hint bulk --bytes "$BYTES" --runs 3 \
    >/tmp/prof-c.log 2>&1 &

sleep 3
CLI_PID=""; SRV_PID=""
for p in $(pgrep -x raptorpath); do
    if grep -qa -- '--client' /proc/$p/cmdline 2>/dev/null; then CLI_PID=$p; fi
    if grep -qa -- '--server' /proc/$p/cmdline 2>/dev/null; then SRV_PID=$p; fi
done
echo "CLI_PID=$CLI_PID SRV_PID=$SRV_PID"
[[ -z "$CLI_PID" ]] && { echo "no client pid"; cat /tmp/prof-c.log; exit 1; }

read_cpu() { awk '{print $14+$15}' /proc/$1/stat 2>/dev/null || echo 0; }
HZ=$(getconf CLK_TCK)
c0=$(read_cpu $CLI_PID); s0=$(read_cpu $SRV_PID); t0=$(date +%s.%N)

echo "profiling sender pid=$CLI_PID for ${PROF_S}s ..."
perf record -F 999 -g --call-graph fp -o /tmp/prof-c.data -p $CLI_PID -- sleep $PROF_S 2>/tmp/perf-rec.log
perf record -F 999 -g --call-graph fp -o /tmp/prof-s.data -p $SRV_PID -- sleep 4 2>/dev/null || true

t1=$(date +%s.%N); c1=$(read_cpu $CLI_PID); s1=$(read_cpu $SRV_PID)
dt=$(echo "$t1 - $t0" | bc -l)
cli_cpu=$(echo "scale=1; ($c1 - $c0) / $HZ / $dt * 100" | bc -l)
srv_cpu=$(echo "scale=1; ($s1 - $s0) / $HZ / $dt * 100" | bc -l)
echo "=== CPU over ${dt}s: CLIENT(sender)=${cli_cpu}%  SERVER(recv)=${srv_cpu}%  (100%=1 core) ==="

echo "=== SENDER perf report (top self) ==="
perf report -i /tmp/prof-c.data --stdio -n --percent-limit 0.5 2>/dev/null \
    | grep -E '^\s+[0-9]' | head -30
echo "=== RECEIVER perf report (top self) ==="
perf report -i /tmp/prof-s.data --stdio -n --percent-limit 0.5 2>/dev/null \
    | grep -E '^\s+[0-9]' | head -20
echo "=== client log summary ==="
grep -E 'summary|warmup' /tmp/prof-c.log | tail -4
