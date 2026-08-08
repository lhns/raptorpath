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
#
# feat/gen-on-rebaseline NAME-COLLISION NOTE: the binary reads RWM_GEN as the
# generation SIZE G (net/mod.rs:1303/1458/3286, default 384).  This harness ALSO
# uses RWM_GEN as the on/off GATE for --window-generation-coding (see GEN_FLAG
# below): RWM_GEN=0 -> plain-window control, unset/1 -> generation ON at default G.
# To keep both meanings we forward RWM_GEN to the binary as a SIZE only when it is
# a REAL generation size (>=2); the gate sentinels 0 and 1 are NOT forwarded (=1
# would otherwise set a catastrophic 1-symbol generation, =0 a 0-symbol one).
if [[ -n "${RWM_GEN:-}" && "${RWM_GEN}" != "0" && "${RWM_GEN}" != "1" ]]; then
    TENV="$TENV RWM_GEN=$RWM_GEN"
fi
[[ -n "${RWM_PIPELINE:-}" ]] && TENV="$TENV RWM_PIPELINE=$RWM_PIPELINE"
[[ -n "${RWM_GEN_R:-}" ]] && TENV="$TENV RWM_GEN_R=$RWM_GEN_R"
# Proactive-FEC-vs-ARQ crossover knobs: BDP-scaled store/in-flight window,
# coded pacing ceiling, and the proactive-vs-reactive fraction trace.
[[ -n "${RWM_STORE:-}" ]] && TENV="$TENV RWM_STORE=$RWM_STORE"
# Path-scaled outstanding pool (task #84, feat/recv-parallel): the plain-
# reliable multipath store-starvation fix — cap = clamp(gain·N·pipe, floor,
# N·pool) for N>=2 live paths; N=1 = legacy bit-exact. DEFAULT ON since
# 2026-07-21 (goal-gate "Consolidation"); =0 is the legacy-pool opt-out arm.
[[ -n "${RWM_STORE_PATHS:-}" ]] && TENV="$TENV RWM_STORE_PATHS=$RWM_STORE_PATHS"
[[ -n "${RWM_STORE_PATH_POOL:-}" ]] && TENV="$TENV RWM_STORE_PATH_POOL=$RWM_STORE_PATH_POOL"
# Capacity-weighted shared pool (goal-gate "C8-Aware Pool Law", ADR-0058
# follow-up): pool = sum_i honest per-path cap over live paths for N>=2 with
# warm anchors; fallback = the configured pooled law. Default OFF; the
# battery arm composes RWM_PLAIN_RS=1 (honest anchor terms).
[[ -n "${RWM_STORE_CAPW:-}" ]] && TENV="$TENV RWM_STORE_CAPW=$RWM_STORE_CAPW"
# Per-path outstanding accounting (task #86): cap_i = clamp(gain·rate_i·echoRTT_i,
# floor, pool) per live path for N>=2; supersedes RWM_STORE_PATHS' pooled gate;
# N=1 / unset = legacy byte-identical.
[[ -n "${RWM_STORE_PERCAP:-}" ]] && TENV="$TENV RWM_STORE_PERCAP=$RWM_STORE_PERCAP"
# Roadmap item 1 (#86 c8 fix): delay-aware redirect guard — default ON under
# percap; =0 restores the unguarded redirect (the c8-regression control arm).
[[ -n "${RWM_PERCAP_GUARD:-}" ]] && TENV="$TENV RWM_PERCAP_GUARD=$RWM_PERCAP_GUARD"
# feat/store-borrowing (paper 16.22): bounded account borrowing — a cap-full
# pick flies on its picked pipe charged to the lender, bounded by
# lend_i->j <= cap_i - out_i - rate_i*T_return(j). Default OFF; requires percap.
[[ -n "${RWM_STORE_BORROW:-}" ]] && TENV="$TENV RWM_STORE_BORROW=$RWM_STORE_BORROW"
# Residual (iii) flight-witness attribution fix (rides RWM_PLAIN_RS; =0 is the
# legacy last-sent-path control arm).
[[ -n "${RWM_RS_ATTR:-}" ]] && TENV="$TENV RWM_RS_ATTR=$RWM_RS_ATTR"
# feat/anchor-hygiene: the honest plain-mode send-interval BtlBw sampler
# (RWM_PLAIN_RS) and the hygiene umbrella — the honest-anchor measurement arm.
[[ -n "${RWM_PLAIN_RS:-}" ]] && TENV="$TENV RWM_PLAIN_RS=$RWM_PLAIN_RS"
[[ -n "${RWM_ANCHOR_HYGIENE:-}" ]] && TENV="$TENV RWM_ANCHOR_HYGIENE=$RWM_ANCHOR_HYGIENE"
# feat/consolidation: the anchor-pair members of the candidate default stack
# (M* peer-report RTT-feed suppression + the process-clock stall witness) —
# forwarded individually so the leave-one-out arms can key each one.
[[ -n "${RWM_MSTAR_ANCHOR:-}" ]] && TENV="$TENV RWM_MSTAR_ANCHOR=$RWM_MSTAR_ANCHOR"
[[ -n "${RWM_CLOCK_GAP:-}" ]] && TENV="$TENV RWM_CLOCK_GAP=$RWM_CLOCK_GAP"
# feat/window-mtu (goal-gate "Window Decoupling + MTU Scaling"): part 1 —
# window/inflight decoupling at N=1 (head-span gate + stall meter + retention
# backstop, N1-scoped sampling anchor); part 2 — compact v5 DATA framing.
# Both default OFF; forwarded for the pre-registered A/B battery.
[[ -n "${RWM_WIN_DECOUPLE:-}" ]] && TENV="$TENV RWM_WIN_DECOUPLE=$RWM_WIN_DECOUPLE"
[[ -n "${RWM_WIRE_COMPACT:-}" ]] && TENV="$TENV RWM_WIRE_COMPACT=$RWM_WIRE_COMPACT"
# feat/percap-honest-cap: honest store caps under RWM_PLAIN_RS — cap_i =
# anchor_i*(K_i+gain-1) + rate_i*(gain-1)*R (K_i = windowed-min echoSRTT/RTprop,
# R = 100ms recovery-round bound). Default ON whenever RWM_PLAIN_RS is set;
# =0 restores the floor-law control arm.
[[ -n "${RWM_HONEST_CAP:-}" ]] && TENV="$TENV RWM_HONEST_CAP=$RWM_HONEST_CAP"
[[ -n "${RWM_STORE_GAIN:-}" ]] && TENV="$TENV RWM_STORE_GAIN=$RWM_STORE_GAIN"
[[ -n "${RWM_GEN_INFLIGHT:-}" ]] && TENV="$TENV RWM_GEN_INFLIGHT=$RWM_GEN_INFLIGHT"
[[ -n "${RWM_GEN_RATE:-}" ]] && TENV="$TENV RWM_GEN_RATE=$RWM_GEN_RATE"
[[ -n "${RWM_GEN_RATE_FLOOR:-}" ]] && TENV="$TENV RWM_GEN_RATE_FLOOR=$RWM_GEN_RATE_FLOOR"
[[ -n "${RWM_INFL_CAP:-}" ]] && TENV="$TENV RWM_INFL_CAP=$RWM_INFL_CAP"
[[ -n "${RWM_CODED_SRC:-}" ]] && TENV="$TENV RWM_CODED_SRC=$RWM_CODED_SRC"
[[ -n "${RWM_NO_REACTIVE:-}" ]] && TENV="$TENV RWM_NO_REACTIVE=$RWM_NO_REACTIVE"
[[ -n "${RWM_DIAG:-}" ]] && TENV="$TENV RWM_DIAG=$RWM_DIAG"
# Emission batching (goal-gate "Emission Batching", default OFF, sender-only)
# + burst quantum — forwarded for the A/B arms (the embatch session kept this
# forwarding VM-local; landed here by feat/recv-permsg).
[[ -n "${RWM_EMIT_BATCH:-}" ]] && TENV="$TENV RWM_EMIT_BATCH=$RWM_EMIT_BATCH"
[[ -n "${RWM_EMIT_BURST:-}" ]] && TENV="$TENV RWM_EMIT_BURST=$RWM_EMIT_BURST"
# Receiver per-message wall (goal-gate "Receiver Per-Message Wall",
# feat/recv-permsg): estimator heavy-math cadence — the profile-named part.
[[ -n "${RWM_EST_CADENCE:-}" ]] && TENV="$TENV RWM_EST_CADENCE=$RWM_EST_CADENCE"
# Window-mode control-datagram merge (goal-gate "Unlock The Default 1:
# ack-merge" / "Ack-Merge Flip", RWM_ACK_MERGE): the receiver suppresses the
# legacy per-batch Ack and the SACK WindowAck carries its payload. The
# 2026-08-07 battery relied on the implicit `sudo env … → ip netns exec`
# environment inheritance; forwarded EXPLICITLY here so the arm cannot go
# silently inert if that path ever changes (MEASUREMENT DISCIPLINE item 1 —
# the liveness echo is asserted per arm, but a harness that can drop the knob
# is a harness that will).
[[ -n "${RWM_ACK_MERGE:-}" ]] && TENV="$TENV RWM_ACK_MERGE=$RWM_ACK_MERGE"
# Pool-anchor honest dual-store law (goal-gate "Ship The Wins 1",
# feat/ship-est-cadence): the N>=2 store cap on the per-path send-interval
# anchor; default rides RWM_EST_CADENCE — =0 is the est-only decomposition arm.
[[ -n "${RWM_POOL_ANCHOR:-}" ]] && TENV="$TENV RWM_POOL_ANCHOR=$RWM_POOL_ANCHOR"
# Delivery-clocked pool rate anchor (goal-gate "Ship The Wins 1b" arm A,
# feat/pool-delivery-anchor): the N>=2 pool law's rate input gains the shadow
# DeliveryRateAnchor term; default rides RWM_POOL_ANCHOR -- =0 reproduces
# attempt 1 exactly. And arm B: the honest anchor-floor bound (default OFF).
[[ -n "${RWM_POOL_DELIV:-}" ]] && TENV="$TENV RWM_POOL_DELIV=$RWM_POOL_DELIV"
[[ -n "${RWM_FLOOR_BOUND:-}" ]] && TENV="$TENV RWM_FLOOR_BOUND=$RWM_FLOOR_BOUND"
# Engine-receiver saturation probe (roadmap item 2, feat/engine-parallel):
# busy% + inbound msg-queue depth on the RECEIVER (server log /tmp/rwm-s.log).
# (RWM_ENGINE_PAR itself was NOT built -- the item-2 profile refuted it; see
# goal-gate "Engine Parallelization".)
[[ -n "${RWM_RDIAG:-}" ]] && TENV="$TENV RWM_RDIAG=$RWM_RDIAG"
# r* burst-tail provisioning (task #46, paper 8.4.1): RWM_RSTAR_TAIL=0 is the
# legacy GE-only r* arm (same-binary A/B); unset/1 = shipped tail-provisioned r*.
[[ -n "${RWM_RSTAR_TAIL:-}" ]] && TENV="$TENV RWM_RSTAR_TAIL=$RWM_RSTAR_TAIL"
# #85 budget-conserving taper emission (task #85, fix/taper-emission): plain-mode
# proactive repair budgeted at r x source per coding window (legacy = r per ack
# cycle). Default OFF in the binary; forwarded for the queued L1 2x2 spot check.
[[ -n "${RWM_TAPER_R:-}" ]] && TENV="$TENV RWM_TAPER_R=$RWM_TAPER_R"
# feat/recovery-suppression: multipath recovery suppression (the fifth-wall
# lever, goal-gate 16.23 successor) — per-flight RFC9002-style hole law +
# per-path batch serial namespaces. DEFAULT ON since 2026-07-21 (goal-gate
# "Consolidation"); =0 is the legacy opt-out arm; sub-gates _LAW/_SERIAL are
# the trace-attribution probe arms.
[[ -n "${RWM_RECOV_MP:-}" ]] && TENV="$TENV RWM_RECOV_MP=$RWM_RECOV_MP"
[[ -n "${RWM_RECOV_MP_LAW:-}" ]] && TENV="$TENV RWM_RECOV_MP_LAW=$RWM_RECOV_MP_LAW"
[[ -n "${RWM_RECOV_MP_SERIAL:-}" ]] && TENV="$TENV RWM_RECOV_MP_SERIAL=$RWM_RECOV_MP_SERIAL"
# Task #61 (paper 16.20) unified RLC-family machine: RWM_UNIFIED=1 = one global
# sparse-aware decoder for both wires + the A*/M*/Delta span law (Realtime rides
# the RLC family; gen mode defaults the M* depth law ON). Default OFF in the
# binary = legacy byte-identical; forwarded for the queued L1 parity battery.
[[ -n "${RWM_UNIFIED:-}" ]] && TENV="$TENV RWM_UNIFIED=$RWM_UNIFIED"
# goal-gate "Unified Shedding" (fix C): the δ-honest overload-shed sub-gate
# (=0 = serializing control arm) and the A* anchor opt-out (default ON under
# RWM_UNIFIED since this branch).
[[ -n "${RWM_UNIFIED_SHED:-}" ]] && TENV="$TENV RWM_UNIFIED_SHED=$RWM_UNIFIED_SHED"
[[ -n "${RWM_ASTAR_ANCHOR:-}" ]] && TENV="$TENV RWM_ASTAR_ANCHOR=$RWM_ASTAR_ANCHOR"
# Per-run completion timeout override (reliability batteries where DNF is an
# expected datum; see src/perf.rs run_timeout()).
[[ -n "${RWM_PERF_TIMEOUT_S:-}" ]] && TENV="$TENV RWM_PERF_TIMEOUT_S=$RWM_PERF_TIMEOUT_S"
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
# SACK-clocked store release (feat/bbr-default-and-store-release, goal-gate
# "SACK-Clocked Store Release"): SACKed seqs uncounted from the outstanding
# gate, payload + ARQ maps retained until the cumulative frontier — slot
# release, never recoverability (the RWM_SACK_PRUNE distinction).
[[ -n "${RWM_STORE_SACK_RELEASE:-}" ]] && TENV="$TENV RWM_STORE_SACK_RELEASE=$RWM_STORE_SACK_RELEASE"
# (RWM_FMTCP pass-through removed 2026-07-27: the composite was deleted per the
# DEPRECATION REGISTER after the C8-Aware Pool Law re-test CONFIRMED-REFUTED it.)
[[ -n "${RWM_DAPS:-}" ]] && TENV="$TENV RWM_DAPS=$RWM_DAPS"
# DAPS queue management (feat/daps-queue-mgmt): BLEST per-path in-flight BDP cap
# (RWM_DAPS_BDP=gain, default 1.0) + BBR per-path pacing (RWM_DAPS_PACE=0 off).
[[ -n "${RWM_DAPS_BDP:-}" ]] && TENV="$TENV RWM_DAPS_BDP=$RWM_DAPS_BDP"
[[ -n "${RWM_DAPS_PACE:-}" ]] && TENV="$TENV RWM_DAPS_PACE=$RWM_DAPS_PACE"
# Pace-all-traffic (feat/pace-all-traffic): pace the CODED/REPAIR emission through
# the SAME per-path BtlBw pacer as source (on by default under DAPS pacing;
# RWM_PACE_ALL=0 reproduces the source-only pacer — the same-binary A/B baseline).
[[ -n "${RWM_PACE_ALL:-}" ]] && TENV="$TENV RWM_PACE_ALL=$RWM_PACE_ALL"
# Source-backpressure (feat/source-backpressure): bound the SOURCE emission by
# the per-path BtlBw bucket too — DEFER (pause the TUN read) instead of spilling
# the fast bucket negative (on by default under DAPS pacing; RWM_SRC_BP=0
# reproduces the pre-backpressure spill — the same-binary A/B baseline).
[[ -n "${RWM_SRC_BP:-}" ]] && TENV="$TENV RWM_SRC_BP=$RWM_SRC_BP"
# Per-path estimator standalone (feat/per-path-estimator): establish per-path
# BtlBw in a PLAIN generation multipath run (no DAPS) for the general-fix check.
[[ -n "${RWM_PER_PATH_EST:-}" ]] && TENV="$TENV RWM_PER_PATH_EST=$RWM_PER_PATH_EST"
# BBR rate-sample anchor (feat/btlbw-rate-sample): send-interval delivery-rate
# sampling (ack-aggregation robust); ON by default under the per-path estimator.
# RWM_RATE_SAMPLE=0 reproduces the legacy ack-interval anchor (same-binary A/B).
[[ -n "${RWM_RATE_SAMPLE:-}" ]] && TENV="$TENV RWM_RATE_SAMPLE=$RWM_RATE_SAMPLE"
# DAPS read-ahead depth bound (feat/daps-readahead-depth): bound each non-fastest
# path's read-ahead to skew·BtlBw_j (queue delay <= skew); ON by default under
# DAPS+rate-sample. RWM_DAPS_DEPTH=0 reproduces the unbounded read-ahead (A/B).
[[ -n "${RWM_DAPS_DEPTH:-}" ]] && TENV="$TENV RWM_DAPS_DEPTH=$RWM_DAPS_DEPTH"
# Gen-substrate ceiling (feat/gen-substrate-ceiling): derived pipeline depth
# M* + per-path BDP in-flight cap + sent-clock target + windowed-max pace
# (RWM_GEN_PIPE=1), and the QUIC substrate congestion-controller override
# (RWM_QUIC_CC=bbr|newreno|cubic) — the per-path ~10 Mbit/s wall A/B levers.
[[ -n "${RWM_GEN_PIPE:-}" ]] && TENV="$TENV RWM_GEN_PIPE=$RWM_GEN_PIPE"
[[ -n "${RWM_QUIC_CC:-}" ]] && TENV="$TENV RWM_QUIC_CC=$RWM_QUIC_CC"
# MTU floor (fix/frontier-wedge): min_mtu=initial_mtu=1350 so quinn's MTU
# black-hole reset can never drop max_datagram_size below a symbol datagram
# (the ~60 s c3/C8 collapse-run mechanism). Default ON in the binary;
# RWM_MTU_FLOOR=0 restores stock quinn MTUD (the wedge-reproduction arm).
[[ -n "${RWM_MTU_FLOOR:-}" ]] && TENV="$TENV RWM_MTU_FLOOR=$RWM_MTU_FLOOR"

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
[[ "${RWM_GEN:-1}" == "0" ]] && GEN_FLAG=""
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
