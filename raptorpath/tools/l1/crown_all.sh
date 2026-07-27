#!/bin/bash
# Streaming Crown Re-Test orchestrator: env header + stage sequence.
#   sudo bash crown_all.sh [outdir]
# Stages (per goal-gate pre-registration): crown s42, crown s7, l2shape s42,
# l2shape s7, bulk s42. Runtimes stated per stage.
set -uo pipefail
cd "$(dirname "$0")"
OUT="${1:-/home/vibe/crown}"
mkdir -p "$OUT"
BIN="/home/vibe/raptorpath/target/release/raptorpath"

{
echo "==== STREAMING CROWN RE-TEST $(date -u +%FT%TZ) host=$(hostname) ===="
echo "commit: $(cd /home/vibe/raptorpath && git rev-parse HEAD 2>/dev/null || echo 'synced-tree (no git)')"
echo "binary: $BIN sha256=$(sha256sum "$BIN" | cut -d' ' -f1)"
echo "kernel: $(uname -r)"
lscpu | grep -E 'Model name|Flags' | sed 's/Flags:.*\(aes\)/Flags(grep): \1/' | head -2
lscpu | grep -oE 'aes|avx2|pclmulqdq' | sort -u | tr '\n' ' '; echo "(crypto/simd flags)"
python3 --version
echo "env: $(env | grep -E '^RWM_|^SEED' || echo '<no RWM_/SEED set>')"
} | tee "$OUT/env.log"

run_stage() {
    local seed="$1" stage="$2" t0 t1
    t0=$(date +%s)
    echo "---- stage $stage seed $seed START $(date -u +%T)" | tee -a "$OUT/run.log"
    bash ./crown_battery.sh "$seed" "$stage" "$OUT" 2>&1 | tail -8 | tee -a "$OUT/run.log"
    t1=$(date +%s)
    echo "---- stage $stage seed $seed DONE $(date -u +%T) runtime=$(( (t1-t0)/60 ))m$(( (t1-t0)%60 ))s" | tee -a "$OUT/run.log"
}

run_stage 42 crown
run_stage 7  crown
run_stage 42 l2shape
run_stage 7  l2shape
run_stage 42 bulk
echo "==== ALL STAGES DONE $(date -u +%FT%TZ) ====" | tee -a "$OUT/run.log"
