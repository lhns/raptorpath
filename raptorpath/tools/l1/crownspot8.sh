#!/bin/bash
# The crown no-regression spot: the realtime tail matrix on the shipped
# defaults (result in docs/status.md §2).
#
#   bash crownspot8.sh          # run as `vibe`, unprivileged; sudo is taken
#                               # per stage, inside, so the sentinels are
#                               # written by the level that reads them.
#
# One arm — `ship` (env unset = today's defaults) — on `tail_matrix.sh`'s own
# default seat: `--protocol-hint realtime` without `--window-reliable`, which
# is the rho < 1 EVICT seat. Cells c2 and c3, sizes 400 B and 1200 B, x8 reps,
# seeds 42 and 7. The EVICT waste scrape rides along inside tail_matrix.sh.
#
# Sentinels are earned, not announced. `DONE-S<seed>` is written only if that
# seed's log exists, is non-empty, and carries its own `CROWNSPOT-DONE` line;
# `DONE-ALL` only if both seeds earned theirs. Every absolute path is
# write+unlink probed before the first measurement (writability is proven at
# launch, not at exit).
set -uo pipefail

OUT="/home/vibe/crownspot8"
HERE="$(cd "$(dirname "$0")" && pwd)"
CELLS="${CROWNSPOT_CELLS:-c2 c3}"
REPS="${CROWNSPOT_REPS:-8}"
SEEDS="${CROWNSPOT_SEEDS:-42 7}"

mkdir -p "$OUT" || { echo "CROWNSPOT ABORT-SENTINEL-UNWRITABLE mkdir $OUT"; exit 2; }

# ── ABORT-SENTINEL-UNWRITABLE: prove every absolute path, before anything ──
for f in DONE-ALL FAILED-ALL DONE-S42 FAILED-S42 DONE-S7 FAILED-S7 \
         crown-s42.log crown-s7.log run.out; do
    if ! ( : > "$OUT/.probe-$f" ) 2>/dev/null; then
        echo "CROWNSPOT ABORT-SENTINEL-UNWRITABLE $OUT/$f"
        exit 2
    fi
    rm -f "$OUT/.probe-$f"
done
echo "CROWNSPOT sentinel-probe OK $(date -u +%FT%TZ)"

# Stale sentinels from an earlier attempt must never be read as this one's.
rm -f "$OUT/DONE-ALL" "$OUT/FAILED-ALL" \
      "$OUT/DONE-S42" "$OUT/FAILED-S42" "$OUT/DONE-S7" "$OUT/FAILED-S7"

echo "CROWNSPOT start $(date -u +%FT%TZ) cells='$CELLS' reps=$REPS seeds='$SEEDS'"
sha256sum /home/vibe/raptorpath/target/release/raptorpath || true

ALL_OK=1
for seed in $SEEDS; do
    LEDGER="$OUT/crown-s${seed}.log"
    : > "$LEDGER"
    SEED_OK=1
    for cell in $CELLS; do
        echo "=== CROWNSPOT stage seed=$seed cell=$cell start=$(date -u +%FT%TZ)" >> "$LEDGER"
        sudo -n env RWM_GEN=0 RWM_DIAG=1 RWM_TM_ARMS=ship SEED="$seed" \
            bash "$HERE/tail_matrix.sh" "$cell" "$REPS" >> "$LEDGER" 2>&1
        rc=$?
        echo "=== CROWNSPOT stage seed=$seed cell=$cell rc=$rc end=$(date -u +%FT%TZ)" >> "$LEDGER"
        if [[ $rc -ne 0 ]]; then
            echo "CROWNSPOT ABORT-RC seed=$seed cell=$cell rc=$rc"
            SEED_OK=0
        fi
    done
    # Earned, not announced: the log must exist, be non-empty, and carry a
    # completed matrix for every cell before the seed's sentinel is written.
    got=$({ grep -c '^=== done' "$LEDGER" || true; })
    want=$(echo $CELLS | wc -w)
    if [[ $SEED_OK -eq 1 && -s "$LEDGER" && "$got" == "$want" ]]; then
        echo "CROWNSPOT-DONE seed=$seed stages=$got/$want $(date -u +%FT%TZ)" >> "$LEDGER"
        echo "seed=$seed stages=$got/$want $(date -u +%FT%TZ)" > "$OUT/DONE-S${seed}"
    else
        echo "seed=$seed ok=$SEED_OK stages=$got/$want $(date -u +%FT%TZ)" > "$OUT/FAILED-S${seed}"
        ALL_OK=0
    fi
done

if [[ $ALL_OK -eq 1 && -f "$OUT/DONE-S42" && -f "$OUT/DONE-S7" ]]; then
    echo "crownspot v8 complete $(date -u +%FT%TZ)" > "$OUT/DONE-ALL"
    echo "CROWNSPOT DONE-ALL $(date -u +%FT%TZ)"
else
    echo "crownspot v8 incomplete $(date -u +%FT%TZ)" > "$OUT/FAILED-ALL"
    echo "CROWNSPOT FAILED-ALL $(date -u +%FT%TZ)"
fi
