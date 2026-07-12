#!/bin/bash
# RWM Phase C measurement: `raptorpath perf` (rp-native objects) over the
# RELIABLE sliding-window pipeline, single OR dual path, with an optional
# OUT-OF-ORDER object delivery toggle (paper §16.2 H->inf corner).
#
#   sudo bash perf_rwm_c.sh <scenA> <scenB> <hint> <bytes> <runs> <dual|single> [T]
#
#   env RWM_OOO=1     -> add --window-out-of-order (H->inf, decode-on-total)
#   env RWM_EXTRA=".." -> extra CLI args appended to server+client (raise-r arm)
#   env RWM_PLACE_T=.. -> placement-temperature override (via 7th arg too)
#
#   C7 = c2 c2   C8 = c2 c3
set -uo pipefail
cd "$(dirname "$0")"
source ./lib.sh
BIN="/home/vibe/raptorpath/target/release/raptorpath"
SCENA="${1:?scenA}"; SCENB="${2:?scenB}"; HINT="${3:-bulk}"
BYTES="${4:-1800000}"; RUNS="${5:-10}"; MODE="${6:-dual}"; PLACE_T="${7:-}"

TENV=""
[[ -n "$PLACE_T" ]] && TENV="$TENV RWM_PLACE_T=$PLACE_T"
[[ -n "${RWM_MIN_R:-}" ]] && TENV="$TENV RWM_MIN_R=$RWM_MIN_R"
[[ -n "${RWM_FDIAG:-}" ]] && TENV="$TENV RWM_FDIAG=$RWM_FDIAG"
[[ -n "${RWM_FRONTIER_R:-}" ]] && TENV="$TENV RWM_FRONTIER_R=$RWM_FRONTIER_R"
[[ -n "${RWM_FRONTIER:-}" ]] && TENV="$TENV RWM_FRONTIER=$RWM_FRONTIER"
[[ -n "${RWM_FRONTIER_OFFSET:-}" ]] && TENV="$TENV RWM_FRONTIER_OFFSET=$RWM_FRONTIER_OFFSET"
[[ -n "${RWM_FRONTIER_GAIN:-}" ]] && TENV="$TENV RWM_FRONTIER_GAIN=$RWM_FRONTIER_GAIN"
[[ -n "${RWM_WINDOW:-}" ]] && TENV="$TENV RWM_WINDOW=$RWM_WINDOW"
# Generation-coding knobs (§16.3): G, pipeline depth M, per-generation overhead
# r. Propagated into the netns exec env so both server and client see them.
[[ -n "${RWM_GEN:-}" ]] && TENV="$TENV RWM_GEN=$RWM_GEN"
[[ -n "${RWM_PIPELINE:-}" ]] && TENV="$TENV RWM_PIPELINE=$RWM_PIPELINE"
[[ -n "${RWM_GEN_R:-}" ]] && TENV="$TENV RWM_GEN_R=$RWM_GEN_R"
# Proactive-FEC-vs-ARQ crossover knobs: BDP-scaled store/in-flight window,
# coded pacing ceiling, and the proactive-vs-reactive fraction trace.
[[ -n "${RWM_STORE:-}" ]] && TENV="$TENV RWM_STORE=$RWM_STORE"
[[ -n "${RWM_GEN_INFLIGHT:-}" ]] && TENV="$TENV RWM_GEN_INFLIGHT=$RWM_GEN_INFLIGHT"
[[ -n "${RWM_GEN_RATE:-}" ]] && TENV="$TENV RWM_GEN_RATE=$RWM_GEN_RATE"
[[ -n "${RWM_GEN_RATE_FLOOR:-}" ]] && TENV="$TENV RWM_GEN_RATE_FLOOR=$RWM_GEN_RATE_FLOOR"
[[ -n "${RWM_INFL_CAP:-}" ]] && TENV="$TENV RWM_INFL_CAP=$RWM_INFL_CAP"
[[ -n "${RWM_CODED_SRC:-}" ]] && TENV="$TENV RWM_CODED_SRC=$RWM_CODED_SRC"
[[ -n "${RWM_NO_REACTIVE:-}" ]] && TENV="$TENV RWM_NO_REACTIVE=$RWM_NO_REACTIVE"
[[ -n "${RWM_DIAG:-}" ]] && TENV="$TENV RWM_DIAG=$RWM_DIAG"
[[ -n "${RWM_PFRAC:-}" ]] && TENV="$TENV RWM_PFRAC=$RWM_PFRAC"
[[ -n "${RWM_TRACE:-}" ]] && TENV="$TENV RWM_TRACE=$RWM_TRACE"
# Transport-substrate fixes (feat/transport-substrate): CC-rate source pacing
# (Fix 1), bounded reactive under CC (Fix 2), OOO retention decouple (Fix 3).
[[ -n "${RWM_CC_PACE:-}" ]] && TENV="$TENV RWM_CC_PACE=$RWM_CC_PACE"
[[ -n "${RWM_CC_PACE_HR:-}" ]] && TENV="$TENV RWM_CC_PACE_HR=$RWM_CC_PACE_HR"
[[ -n "${RWM_REACT_CAP:-}" ]] && TENV="$TENV RWM_REACT_CAP=$RWM_REACT_CAP"
[[ -n "${RWM_OOO_RETAIN:-}" ]] && TENV="$TENV RWM_OOO_RETAIN=$RWM_OOO_RETAIN"
# Receiver-tail parallelization (feat/receiver-tail): PART 1 report-all-deficits
# (RWM_REPORT_GENS) + PART 1.2 BDP-derived in-flight cap (RWM_INFL_BDP).
[[ -n "${RWM_REPORT_GENS:-}" ]] && TENV="$TENV RWM_REPORT_GENS=$RWM_REPORT_GENS"
[[ -n "${RWM_INFL_BDP:-}" ]] && TENV="$TENV RWM_INFL_BDP=$RWM_INFL_BDP"
# Repair-coverage horizon (feat/nack-timing): delay the reactive NACK/deficit
# by ~a generation-span so the in-flight proactive repair can decode the hole
# first (FEC-before-ARQ discipline). Milliseconds; 0/unset = report immediately.
[[ -n "${RWM_REPAIR_WAIT:-}" ]] && TENV="$TENV RWM_REPAIR_WAIT=$RWM_REPAIR_WAIT"
# Present-at-stall proactive pacer (feat/present-at-stall): filling-generation
# proactive repair on the generation grid, independent of source/ack-clock.
[[ -n "${RWM_PROACTIVE_PACER:-}" ]] && TENV="$TENV RWM_PROACTIVE_PACER=$RWM_PROACTIVE_PACER"
# Cross-path repair placement (feat/c8-crosspath-repair): route proactive/deficit
# repair to the max-spare-capacity path (the slow path once fast is source-
# saturated) so repair does not displace fast-path systematic source.
[[ -n "${RWM_XPATH_REPAIR:-}" ]] && TENV="$TENV RWM_XPATH_REPAIR=$RWM_XPATH_REPAIR"
# SACK sender-decoupling + BDP reassembly (feat/sack-bdp-reassembly): prune the
# sent-store on any out-of-order ack (RWM_SACK_PRUNE) + clamp the receiver prune
# to the delivered frontier so no pruned symbol is evicted before use, with a
# [REASM] occupancy probe (RWM_REASM_BDP).
[[ -n "${RWM_SACK_PRUNE:-}" ]] && TENV="$TENV RWM_SACK_PRUNE=$RWM_SACK_PRUNE"
[[ -n "${RWM_REASM_BDP:-}" ]] && TENV="$TENV RWM_REASM_BDP=$RWM_REASM_BDP"
# FMTCP-class pure decode-on-total aggregation (feat/fmtcp-aggregation): the
# composite gate — total-in-flight flow control + per-path BDP in-flight cap +
# fungible fountain repair (no per-hole ARQ) + decode-on-total OOO. Self-selects
# the systematic-repair generation submode on top of --window-reliable.
[[ -n "${RWM_FMTCP:-}" ]] && TENV="$TENV RWM_FMTCP=$RWM_FMTCP"
[[ -n "${RWM_DAPS:-}" ]] && TENV="$TENV RWM_DAPS=$RWM_DAPS"

OOO_FLAG=""
[[ "${RWM_OOO:-0}" == "1" ]] && OOO_FLAG="--window-out-of-order"
EXTRA="${RWM_EXTRA:-}"

cleanup() {
    pkill -x raptorpath 2>/dev/null || true
    bash ./topo_dual.sh down >/dev/null 2>&1 || true
}
trap cleanup EXIT
cleanup

if pgrep -x raptorpath >/dev/null 2>&1; then
    echo "BUSY: raptorpath already running -- aborting" >&2
    exit 3
fi

bash ./topo_dual.sh up "$SCENA" "$SCENB" --seed 42 >/dev/null 2>&1

if [[ "$MODE" == "dual" ]]; then
    SRV_BIND="10.77.0.2:7000,10.78.0.2:7000"
    PEERS="10.77.0.2:7000,10.78.0.2:7000"
    CLI_BIND="10.77.0.1:0,10.78.0.1:0"
else
    SRV_BIND="10.77.0.2:7000"
    PEERS="10.77.0.2:7000"
    CLI_BIND="10.77.0.1:0"
fi

ip netns exec "$NS_SRV" env $TENV "$BIN" perf --server --bind "$SRV_BIND" \
    --window-reliable $OOO_FLAG $EXTRA --protocol-hint "$HINT" >/tmp/rwm-s.log 2>&1 &

for _ in $(seq 1 20); do
    ip netns exec "$NS_SRV" ss -uln 2>/dev/null | grep -q ':7000' && break
    sleep 0.3
done
sleep 1

echo "--- RWM-C perf mode=$MODE hint=$HINT A=$SCENA B=$SCENB ooo=${RWM_OOO:-0} extra='$EXTRA' T=${PLACE_T:-default} ($BYTES x $RUNS) start=$(date +%T)"
timeout 700 ip netns exec "$NS_CLI" env $TENV "$BIN" perf --client \
    --peer "$PEERS" --bind "$CLI_BIND" \
    --window-reliable $OOO_FLAG $EXTRA --protocol-hint "$HINT" \
    --bytes "$BYTES" --runs "$RUNS" 2>&1 | tee /tmp/rwm-c.log \
    | grep -E "summary|warmup|dnf|PFRAC" | tail -8 \
    || echo "{\"dnf\":true,\"mode\":\"$MODE\"}"
echo "    done $(date +%T)"
echo "--- server log tail:"; sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-s.log | tail -3
