#!/bin/bash
# Streaming Crown Re-Test battery (meas/streaming-retirement, 2026-07-27).
# The DEPRECATION REGISTER's streaming-machine re-test clause, stage 1 of the
# VISION-TRIAGE retirement path: does the SHIPPED DEFAULT (unified machine,
# env unset) hold the historic 12-48x message-tail crown record cell-by-cell
# against the streaming machine (RWM_UNIFIED=0, the legacy opt-out)?
#
#   sudo bash crown_battery.sh <seed> <stage> <outdir>
#     stage: crown   = cells 1-4 (tail_matrix c2+c3 realtime 400/1200B,
#                      50msg/s x 20s), 8 rounds x REPS=1, arms interleaved
#                      per rep with order alternating per round
#            l2shape = cell 5 (c2 realtime 1200B, 50msg/s x 30s = the L2-era
#                      stream_bench shape, 1500 msgs, p999 gated), 5 rounds
#            bulk    = sanity spot S (c2 bulk 400/1200B, block pipeline,
#                      streaming-inert control), 4 rounds
#
# MEASUREMENT DISCIPLINE: same binary every arm (sha256 in the header via
# crown_all.sh), fresh warm tunnel per rep-arm, per-rep p50/p99/p999/max/n,
# liveness echoes per arm both endpoints (asserted by ARMCOUNT/LIVENESS
# below), no captured result discarded, aborts counted loudly.
set -uo pipefail
cd "$(dirname "$0")"

SEED="${1:?seed}"; STAGE="${2:?stage}"; OUT="${3:?outdir}"
mkdir -p "$OUT"
LOG="$OUT/${STAGE}-s${SEED}.log"

case "$STAGE" in
    crown)   CELLS="c2 c3"; ROUNDS=8; ARM_A="streaming"; ARM_B="ship"
             EXTRA_ENV="" ;;
    l2shape) CELLS="c2";    ROUNDS=5; ARM_A="streaming"; ARM_B="ship"
             EXTRA_ENV="RWM_TM_DUR=30 RWM_TM_SIZES=1200" ;;
    bulk)    CELLS="c2";    ROUNDS=4; ARM_A="bulkstream"; ARM_B="bulkship"
             EXTRA_ENV="" ;;
    *) echo "unknown stage '$STAGE'" >&2; exit 1 ;;
esac

{
echo "=== CROWN BATTERY stage=$STAGE seed=$SEED cells='$CELLS' rounds=$ROUNDS start=$(date -u +%FT%TZ)"
echo "=== extra_env='$EXTRA_ENV' arms='$ARM_A $ARM_B' (order alternates per round)"

for cell in $CELLS; do
    for round in $(seq 1 "$ROUNDS"); do
        # Interleave per rep: REPS=1 per invocation, arm order flips each
        # round so neither machine systematically rides session drift.
        if (( round % 2 == 1 )); then ARMS="$ARM_A $ARM_B"; else ARMS="$ARM_B $ARM_A"; fi
        echo "== round $round cell=$cell arms='$ARMS' $(date -u +%T)"
        # shellcheck disable=SC2086
        env SEED="$SEED" RWM_TM_ARMS="$ARMS" $EXTRA_ENV \
            bash ./tail_matrix.sh "$cell" 1 2>&1
    done
done

echo "=== stage done $(date -u +%FT%TZ)"
} >> "$LOG" 2>&1

# ---- Post-stage assertions (discipline items 1 + 7): per-arm captured-rep
# count + liveness echoes, loud on mismatch, never silent. ----
for arm in $ARM_A $ARM_B; do
    n=$(grep -c "  $arm .*B rep1: p50=" "$LOG" || true)
    echo "ARMCOUNT stage=$STAGE seed=$SEED arm=$arm captured_reps=$n" | tee -a "$LOG"
    [[ "$n" -eq 0 ]] && echo "ARMCOUNT FAIL: arm $arm produced ZERO summaries" | tee -a "$LOG"
done
# Liveness: every streaming/bulkstream bringup must echo the streaming
# backend selection (or, for bulk hint, carry RWM_UNIFIED unset semantics);
# every ship bringup must echo the unified span law. Count echo lines.
s_echo=$(grep -c "ECHO ${ARM_A} .*auto-selecting streaming" "$LOG" || true)
u_echo=$(grep -c "ECHO ${ARM_B} .*unified span law ACTIVE" "$LOG" || true)
echo "LIVENESS stage=$STAGE seed=$SEED ${ARM_A}_streaming_echoes=$s_echo ${ARM_B}_unified_echoes=$u_echo" | tee -a "$LOG"
aborts=$(grep -c "BRINGUP_FAIL" "$LOG" || true)
echo "ABORTS stage=$STAGE seed=$SEED bringup_fail=$aborts" | tee -a "$LOG"
