#!/bin/bash
# THE PASSIVE PRIMITIVES — PLAIN-WINDOW PASS (goal #100 item 1, amended).
#
#   sudo bash prim_battery_pw.sh [outdir] [reps]
#
# THE ONE DIFFERENCE FROM `prim_battery.sh` THAT MATTERS: **RWM_GEN=0**.
#
# `prim_battery.sh` left it unset. `perf_rwm_c.sh:74` reads
# `GEN_GATE="${RWM_GEN:-1}"` and `:100` clears `GEN_FLAG` only at `0`, so the
# generation pass ran `--window-generation-coding` — a DIFFERENT MACHINE from
# the one all 41 other drivers, every delta-consuming ledger, and goal #100
# item 2's alpha-sweep run. Under generation `recv_nack_tx = None`
# (net/mod.rs:2434), so `record_fire` is unreachable, `nu = 0` structurally,
# and `d` is the FEC-decode stall because FEC decode is the only recovery
# there is. This pass measures the plain-window machine.
#
# W1-W5 — THE GENERATION-OFF WITNESSES, AND NOT ONE OF THEM IS
# `[GATES] RWM_GEN`. That field formats `self.gen_size` (gates.rs:1007) and is
# emitted byte-identically with the pipeline on or off; it is INERT and this
# harness does not cite it.
#
#   W1  [RFA] gen=0 on the RECEIVER  — the only DIRECT echo of
#       `window_generation` that exists in the engine (main@83db750).
#   W2  no [PFRAC] lines on the sender — perf_rwm_c.sh:104-106 force-sets
#       RWM_PFRAC=1 only when GEN_FLAG is non-empty; the gate defaults OFF.
#   W3  cod=0sym/s in the sender [DIAG] tail — the coded-emission pacer.
#   W4  [DIAG] retx > 0 at every LOSSY cell — the gap-driven retransmit loop.
#   W5  [RACK] fa=<spur>/<fired> present with fired > 0 at every LOSSY cell.
#
# W4+W5 ARE ALSO THE ALPHA-REACHABILITY GATE (MEASUREMENT DISCIPLINE rule 1):
# alpha's two consumers drive the machinery these two counters count. A LOSSY
# rep reading zero on either is VOID, not a small number. `c1` (realised loss
# 0.013 %) is exempt from the lower bound and reports both without a bar.
#
# GOODPUT ABORT BANDS are a SECONDARY, mechanical cross-check: the generation
# plateau is 26.8-34.1 Mbit/s at every cell, the plain-window ledger 78-222.
# Where the band and the witnesses disagree, THE WITNESSES RULE and the
# disagreement is printed.
#
# All gates below are read-only. RWM_WALLDIAG and RWM_LATPROBE are added over
# `prim_battery.sh` so the rows POOL with the ccand/ccap/ladder ledgers, which
# set them.
set -uo pipefail
cd "$(dirname "$0")"

OUT="${1:-/home/vibe/primpw}"
REPS="${2:-3}"
SEED="${RWM_PRIM_SEED:-42}"
CELLS="${RWM_PRIM_CELLS:-c1 c7 c8 c8L sc2}"
BINP="${RWM_BIN:-/home/vibe/raptorpath/target/release/raptorpath}"

mkdir -p "$OUT"

# The committed five, transcribed VERBATIM from `ccand_battery.sh:202-215`.
cell_spec() {
  case "$1" in
    c1)  echo "c1 c1 single 400000000" ;;
    sc2) echo "c2 c2 single 100000000" ;;
    c7)  echo "c2 c2 dual   200000000" ;;
    c8)  echo "c2 c3 dual    25000000" ;;
    c8L) echo "c2 c3 dual   200000000" ;;
    *) echo "" ;;
  esac
}

# ABORT bands, from the committed plain-window ledgers' p05/p95 — except c8L,
# whose own p05 (34.5) lies INSIDE the generation plateau and therefore cannot
# discriminate. Its floor is set at 45 (1.3x the plateau ceiling) and the cost
# is disclosed in the pre-registration rather than the band quietly widened.
band_lo() { case "$1" in c1) echo 147;; c7) echo 140;; c8) echo 50;; c8L) echo 45;; sc2) echo 78;; *) echo 0;; esac; }
band_hi() { case "$1" in c1) echo 294;; c7) echo 180;; c8) echo 100;; c8L) echo 95;; sc2) echo 92;; *) echo 99999;; esac; }
# c1 is the only cell exempt from the W4/W5 lower bound (realised loss 0.013 %).
is_lossy() { [ "$1" != "c1" ] && echo 1 || echo 0; }

echo "== PRIM-PW PASS  seed=$SEED reps=$REPS cells=$CELLS  $(date -u +%FT%TZ)"
echo "== BIN $BINP"
sha256sum "$BINP" 2>/dev/null || true

for cell in $CELLS; do
  spec="$(cell_spec "$cell")"
  if [ -z "$spec" ]; then echo "!! unknown cell $cell" >&2; continue; fi
  set -- $spec
  ca="$1"; cb="$2"; mode="$3"; bytes="$4"
  for r in $(seq 1 "$REPS"); do
    tag="$cell-s$SEED-r$r"
    echo "== RUN $tag  ($ca/$cb $mode $bytes B)  $(date -u +%FT%TZ)"
    env SEED="$SEED" RWM_GEN=0 RWM_DIAG=1 RWM_FDIAG=1 RWM_ACKDIAG=1 \
        RWM_WALLDIAG=1 RWM_LATPROBE=1 \
      bash perf_rwm_c.sh "$ca" "$cb" bulk "$bytes" 1 "$mode" \
      > "$OUT/$tag-run.log" 2>&1
    rc=$?

    cp /tmp/rwm-c.log "$OUT/$tag-c.log" 2>/dev/null || true
    cp /tmp/rwm-s.log "$OUT/$tag-s.log" 2>/dev/null || true
    cp /tmp/rwm-q.txt "$OUT/$tag-q.txt" 2>/dev/null || true

    C="$OUT/$tag-c.log"; S="$OUT/$tag-s.log"
    gc=$(grep -c '\[GATES\]' "$C" 2>/dev/null || echo 0)
    gs=$(grep -c '\[GATES\]' "$S" 2>/dev/null || echo 0)
    fd=$(grep -c '\[FDIAG\]' "$S" 2>/dev/null || echo 0)
    dg=$(grep -c 'sig_us=' "$C" 2>/dev/null || echo 0)
    # W1: the RECEIVER's [RFA] gen= field — the only direct window_generation echo.
    w1=$(grep -o '\[RFA\] gen=[01]' "$S" 2>/dev/null | tail -1 | sed 's/.*gen=//'); w1="${w1:-none}"
    rfa_n=$(grep -c '\[RFA\]' "$S" 2>/dev/null || echo 0)
    # W2: [PFRAC] presence on the sender IS generation.
    w2=$(grep -c '\[PFRAC\]' "$C" 2>/dev/null || echo 0)
    # W3: the coded-emission pacer in the DIAG tail.
    w3=$(grep -o 'cod=[0-9]*sym/s' "$C" 2>/dev/null | tail -1 | tr -dc '0-9'); w3="${w3:-NA}"
    # W4: the gap-driven retransmit loop.
    w4=$(grep -o 'retx=[0-9]*' "$C" 2>/dev/null | tail -1 | tr -dc '0-9'); w4="${w4:-NA}"
    # W5: record_fire's only call site.
    w5=$(grep -o '\[RACK\].*fa=[0-9]*/[0-9]*' "$C" 2>/dev/null | tail -1 | sed 's/.*fa=//'); w5="${w5:-none}"
    mb=$(grep -o '"mean_mbps":[0-9.]*' "$C" 2>/dev/null | tail -1 | sed 's/.*://'); mb="${mb:-0}"
    lo=$(band_lo "$cell"); hi=$(band_hi "$cell"); lossy=$(is_lossy "$cell")
    inband=$(awk -v m="$mb" -v l="$lo" -v h="$hi" 'BEGIN{print (m>=l && m<=h)?1:0}')

    echo "PRIMPWWITNESS {\"cell\":\"$cell\",\"seed\":$SEED,\"rep\":$r,\"rc\":$rc,\"mbps\":$mb,\"band\":[$lo,$hi],\"in_band\":$inband,\"lossy\":$lossy,\"gates_cli\":$gc,\"gates_srv\":$gs,\"fdiag_lines\":$fd,\"sig_lines\":$dg,\"rfa_lines\":$rfa_n,\"W1_rfa_gen\":\"$w1\",\"W2_pfrac_lines\":$w2,\"W3_cod\":\"$w3\",\"W4_retx\":\"$w4\",\"W5_rack_fa\":\"$w5\"}" \
      | tee -a "$OUT/primpw-witness-s$SEED.jsonl"
  done
done

echo "== DONE $(date -u +%FT%TZ)"
touch "$OUT/DONE-PRIMPW-s$SEED"
