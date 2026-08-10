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

# ── GATE FORWARDING (goal-gate "Gate-Forwarding Audit", 2026-08-09) ──────
# ONE shared list, in lib.sh, sourced above. This block used to be 78 lines
# of hand-rolled `[[ -n "${RWM_X:-}" ]] && TENV="$TENV RWM_X=$RWM_X"`, and
# the audit found 12 engine gates that had never been added to it —
# RWM_ACK_MERGE (found by the 2026-08-08 flip battery) plus RWM_RECOV_SP,
# RWM_RECOV_MP_LIVE, RWM_PLACE_SLACK, RWM_PATIENCE_DERIVED,
# RWM_SIDLE_DERIVED, RWM_SCHED_SNAPSHOT, RWM_STORE_BOOT and the four
# RWM_COPA_* knobs. Those arms all measured correctly ANYWAY, by plain
# process-environment inheritance (MEASURED, PROBE 0 + runs A/B/D of the
# audit sweep: the six non-allowlisted echo-bearing gates fire on BOTH logs
# when set and on NEITHER when unset) — but a harness that CAN drop a knob
# is a harness that will, so the forwarding is now total and explicit.
TENV="$(rwm_forward_env)"
[[ -n "$PLACE_T" ]] && TENV="$TENV RWM_PLACE_T=$PLACE_T"

# feat/gen-on-rebaseline NAME-COLLISION NOTE: the binary reads RWM_GEN as the
# generation SIZE G (gates.rs, default 384, `.max(1)`). This harness ALSO uses
# RWM_GEN as the on/off GATE for --window-generation-coding: RWM_GEN=0 -> plain
# window control, unset/1 -> generation ON at the binary's default G.
#
# The sentinels 0 and 1 must therefore NOT reach the binary (=1 would set a
# catastrophic 1-symbol generation, =0 a 0-symbol one clamped to 1). The old
# code tried to achieve that by OMITTING them from the allowlist — which the
# audit showed does nothing, because the binary inherits this script's whole
# environment regardless. The only way to withhold a var is to remove it from
# OUR OWN environment, which is what `unset` below does; a real generation
# size (>=2) is forwarded normally by rwm_forward_env.
GEN_GATE="${RWM_GEN:-1}"
if [[ "${RWM_GEN:-}" == "0" || "${RWM_GEN:-}" == "1" ]]; then
    unset RWM_GEN
    TENV="$(rwm_forward_env)"
    [[ -n "$PLACE_T" ]] && TENV="$TENV RWM_PLACE_T=$PLACE_T"
fi

OOO_FLAG=""
[[ "${RWM_OOO:-0}" == "1" ]] && OOO_FLAG="--window-out-of-order"
EXTRA="${RWM_EXTRA:-}"

# feat/gen-on-rebaseline: GENERATION is FIRST-CLASS in the aggregation harness.
# The coded/generation pipeline (and therefore DAPS, the per-path rate-sample
# estimator, the read-ahead depth bound, source-backpressure — EVERYTHING the
# §16.11-16.14 arc measured) is enabled ONLY by the --window-generation-coding CLI
# flag: net/mod.rs:701 gates window_generation on
#   window_reliable && (window_generation_coding || window_systematic_repair)
# and RWM_DAPS/RWM_GEN_R/RWM_RATE_SAMPLE only *configure* generation, they do NOT
# *enable* it (`daps = RWM_DAPS && generation`).  The §16.14 diagnosis proved this
# harness NEVER passed that flag, so the entire recent arc ran with the coded path
# DEAD (cod=0).  Generation now DEFAULTS ON here; set RWM_GEN=0 for the plain-window
# control.  --window-reliable is kept (generation requires it, main.rs:302).
GEN_FLAG="--window-generation-coding"
# GEN_GATE, not RWM_GEN: the sentinel branch above `unset`s RWM_GEN so it cannot
# reach the binary as a 1-symbol generation size, so the GATE meaning must be
# read from the saved copy.
[[ "$GEN_GATE" == "0" ]] && GEN_FLAG=""
# Force the cumulative coded-emission counter on so the HARD SANITY GUARD (below)
# can assert cod>0 on the SENDER.  RWM_PFRAC makes run_window_sender print
# "[PFRAC] ... total_coded=N ..." every 500 ms (generation-gated, cheap).
if [[ -n "$GEN_FLAG" && -z "${RWM_PFRAC:-}" ]]; then
    TENV="$TENV RWM_PFRAC=1"
fi

cleanup() {
    pkill -x raptorpath 2>/dev/null || true
    bash ./topo_dual.sh down >/dev/null 2>&1 || true
}
trap cleanup EXIT
cleanup
# The tc capture below writes a FIXED path, so a run that aborts before
# reaching it would leave the PREVIOUS invocation's counters there for the
# caller to copy under this cell's name. Silently attributing one cell's
# wire truth to another is worse than having no capture, so clear it first:
# an absent file is then an unambiguous "this invocation produced none".
rm -f /tmp/rwm-q.txt

if pgrep -x raptorpath >/dev/null 2>&1; then
    echo "BUSY: raptorpath already running -- aborting" >&2
    exit 3
fi

bash ./topo_dual.sh up "$SCENA" "$SCENB" --seed "${SEED:-42}" >/dev/null 2>&1

if [[ "$MODE" == "dual" ]]; then
    SRV_BIND="10.77.0.2:7000,10.78.0.2:7000"
    PEERS="10.77.0.2:7000,10.78.0.2:7000"
    CLI_BIND="10.77.0.1:0,10.78.0.1:0"
else
    SRV_BIND="10.77.0.2:7000"
    PEERS="10.77.0.2:7000"
    CLI_BIND="10.77.0.1:0"
fi

# LOG SOURCES (feat/gen-on-rebaseline; the §16.14 wrong-log trap): the --server is
# the perf RECEIVER of the bulk transfer (its reverse sender loop places ~no source,
# ~no coded — reading sender-side counters here is the §16.14 error) -> /tmp/rwm-s.log.
# The --client is the bulk SENDER; the per-path anchor, pacer, depth budget, and the
# coded-emission counters all live here -> /tmp/rwm-c.log.  Sender-side DIAG (btlbw,
# dbud, cod, eff_pace, ANCHOR ...) MUST be scraped from /tmp/rwm-c.log.
ip netns exec "$NS_SRV" env $TENV "$BIN" perf --server --bind "$SRV_BIND" \
    --window-reliable $GEN_FLAG $OOO_FLAG $EXTRA --protocol-hint "$HINT" >/tmp/rwm-s.log 2>&1 &

for _ in $(seq 1 20); do
    ip netns exec "$NS_SRV" ss -uln 2>/dev/null | grep -q ':7000' && break
    sleep 0.3
done
sleep 1

echo "--- RWM-C perf mode=$MODE hint=$HINT A=$SCENA B=$SCENB ooo=${RWM_OOO:-0} extra='$EXTRA' T=${PLACE_T:-default} ($BYTES x $RUNS) start=$(date +%T)"
# CPU accounting (goal-gate "Decode-CPU Ceiling"): the CLIENT (bulk sender /
# encoder) is wrapped in /usr/bin/time -v; the SERVER's (receiver / decoder)
# cumulative CPU is read from /proc/<pid>/stat right after the transfer, before
# teardown.  Reported as CPUCLI/CPUSRV seconds so utilization = cpu/elapsed.
rm -f /tmp/rwm-cli-time
timeout 700 ip netns exec "$NS_CLI" /usr/bin/time -v -o /tmp/rwm-cli-time env $TENV "$BIN" perf --client \
    --peer "$PEERS" --bind "$CLI_BIND" \
    --window-reliable $GEN_FLAG $OOO_FLAG $EXTRA --protocol-hint "$HINT" \
    --bytes "$BYTES" --runs "$RUNS" 2>&1 | tee /tmp/rwm-c.log \
    | grep -E "summary|warmup|dnf|PFRAC" | tail -8 \
    || echo "{\"dnf\":true,\"mode\":\"$MODE\"}"
SRV_TICKS=0
for P in $(pgrep -x raptorpath); do
    T=$(awk '{print $14+$15}' /proc/$P/stat 2>/dev/null || echo 0)
    SRV_TICKS=$((SRV_TICKS + T))
done
HZ=$(getconf CLK_TCK)
CLI_U=$(grep -oP 'User time \(seconds\): \K[0-9.]+' /tmp/rwm-cli-time 2>/dev/null || echo 0)
CLI_S=$(grep -oP 'System time \(seconds\): \K[0-9.]+' /tmp/rwm-cli-time 2>/dev/null || echo 0)
echo "    CPU: CPUSRV=$(awk "BEGIN{printf \"%.2f\", $SRV_TICKS/$HZ}")s CPUCLI=$(awk "BEGIN{printf \"%.2f\", $CLI_U+$CLI_S}")s (srv=decoder cli=sender; whole-invocation incl warmup)"
echo "    done $(date +%T)"
echo "--- server log tail:"; sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-s.log | tail -3

# diag/lossy-residual (goal-gate "Lossy-Single Residual"): wire-truth qdisc
# counters BEFORE teardown — bytes/pkts that passed netem per direction plus
# its GE drops (the loss realization). Read-only; whole-invocation totals
# (warm-up object is 64 B — negligible). cli*=data direction, srv*=acks.
for DEV in cli0 cli1; do
    ST=$(ip netns exec "$NS_CLI" tc -s qdisc show dev "$DEV" 2>/dev/null | tr '\n' ' ') \
        && [[ -n "$ST" ]] && echo "    QDISC $DEV: $ST"
done
for DEV in srv0 srv1; do
    ST=$(ip netns exec "$NS_SRV" tc -s qdisc show dev "$DEV" 2>/dev/null | tr '\n' ' ') \
        && [[ -n "$ST" ]] && echo "    QDISC $DEV: $ST"
done

# goal-gate "Latency Lever", instrument 1 — TC COUNTERS ON EVERY CELL.
#
# The three-term battery captured tc for 2 of its 9 cells, and its central
# negative result ("the store was occupied to the new limit and throughput
# did not follow") needed exactly one number to be readable: the shaped
# link's utilisation. The flattened `QDISC` lines above ALREADY carry it —
# and `tt_battery.sh`'s grep filter threw them away, and their one-line
# form is not what any parser here reads.
#
# The capture MUST happen inside this script: `trap cleanup EXIT` above
# destroys both namespaces the instant this process returns, so by the time
# a caller regains control the qdiscs are gone. So write the sectioned form
# to a FIXED path and let the caller copy it under its own rep-unique name
# (the `adv_battery.sh` precedent). Callers that do not copy it pay nothing
# but a stale /tmp file.
#
# Banner names match `adv_cells.sh counters` so `bind_analyze.py`'s parser
# reads both without a second dialect. CLI1/SRV1 are NEW — dual cells (c7,
# c8) shape two veth pairs and only the first was ever nameable.
{
    for DEV in cli0 cli1; do
        ip netns exec "$NS_CLI" ip link show "$DEV" >/dev/null 2>&1 || continue
        echo "== ${DEV^^} (data-dir egress: netem or tbf+netem bottleneck)"
        ip netns exec "$NS_CLI" tc -s qdisc show dev "$DEV" 2>/dev/null || true
    done
    for DEV in srv0 srv1; do
        ip netns exec "$NS_SRV" ip link show "$DEV" >/dev/null 2>&1 || continue
        echo "== ${DEV^^} (ack-dir egress)"
        ip netns exec "$NS_SRV" tc -s qdisc show dev "$DEV" 2>/dev/null || true
    done
    echo "== SRV0-INGRESS (policer, when present)"
    ip netns exec "$NS_SRV" tc -s filter show dev srv0 parent ffff: 2>/dev/null || true
    # Wall duration of the shaped window, so utilisation is computable from
    # this file ALONE rather than joined against a RUNTIME line elsewhere.
    echo "== INVOCATION_S ${SECONDS}"
} > /tmp/rwm-q.txt 2>/dev/null || true
echo "    QCAP: /tmp/rwm-q.txt $(wc -l < /tmp/rwm-q.txt 2>/dev/null || echo 0) lines"

# --- HARD SANITY GUARD (feat/gen-on-rebaseline) -----------------------------------
# A measurement where the mechanism under test did not run must FAIL LOUDLY, not
# silently report a number.  When generation is requested (GEN_FLAG set, i.e.
# RWM_GEN!=0), assert that CODED symbols actually flowed on the SENDER.  The sender
# is the --client => /tmp/rwm-c.log (NOT the --server/receiver /tmp/rwm-s.log — the
# §16.14 wrong-log trap).  Coded count = max total_coded over the run's [PFRAC] lines.
if [[ -n "$GEN_FLAG" ]]; then
    CODED=$(sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log 2>/dev/null \
        | grep -oE 'total_coded=[0-9]+' | grep -oE '[0-9]+' | sort -n | tail -1)
    CODED="${CODED:-0}"
    if [[ "$CODED" -le 0 ]]; then
        echo "FATAL: generation requested but cod=0 (mechanism inert) -- NO coded symbols flowed on the sender (/tmp/rwm-c.log). The measured binary ran the coded path DEAD; the numbers above are INVALID. Check that --window-generation-coding is on the wire and RWM_GEN!=0." >&2
        exit 7
    fi
    echo "    GUARD OK: generation ACTIVE on the sender (total_coded=$CODED coded symbols flowed)"
fi
