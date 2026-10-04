#!/bin/bash
# THE V4 VERIFICATION'S 5 h ENVELOPE (docs/status.md §6 pre-registration;
# docs/measurement-discipline.md "The five-hour cap").
#
#   V4_ROOT=/home/vibe/v4 V4_HARD_DEADLINE=<epoch> V4_OLD_SHA=<sha> \
#     setsid nohup bash verify4_run_all.sh > $V4_ROOT/launch.out 2>&1 < /dev/null &
#
# Run from the synced tree ($V4_ROOT/src/raptorpath/tools/l1). STARTED AS
# `vibe`, NOT ROOT; `sudo -n` only for the measurement stages.
#
# ORDER (pre-registered), all under both operator locks taken HERE:
#   1. build + tests on the NEW tree: `cargo build --release`; `cargo test -p
#      raptorpath -p raptorpath-math --release --no-fail-fast --
#      --test-threads=2`; `cargo test --doc -p raptorpath --release`; `cargo
#      test -p raptorpath-wasm`; the gate_suite determinism check (fixed
#      harness x2, and the pre-fix harness x2 for contrast); then `cargo build
#      --release --bin raptorpath` -> $V4_ROOT/bin/new/raptorpath, sha256;
#   2. the OLD binary: $V4_OLD_REUSE if its sha256 is V4_OLD_SHA, else built
#      from $V4_ROOT/old-src (archive of 2e264b7) -> $V4_ROOT/bin/old/raptorpath;
#      a sha other than V4_OLD_SHA is ABORT-OLD-BINARY;
#   3. smoke (one invocation per arm, one tunnel bring-up per binary, one
#      tail_matrix `ship` rep) -> SMOKE-PASS or ABORT-SMOKE; TESTS.txt and
#      the smoke check are then read by the operator, who writes GO or NOGO
#      (<= 40 min; timeout = FAILED-ALL);
#   4. budget -> PLAN.txt; 5. battery (verify4_battery.sh); 6. tunnel cell
#      (tun_bulk.sh); 7. crown (crownspot8.sh); 8. score -> score.txt.
#
# SENTINELS: DONE-ALL only when the battery ledger carries V4-BATTERY-DONE
# and `verify4_parse.py check` returns 0, no truncation happened, and the
# tunnel and crown stages each completed or were cut by the plan.
# FAILED-ALL (cause in all-era.txt); FAILED-ALL-TRUNCATED-5H-BUDGET.
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE" || { echo "ABORT-CD $HERE"; exit 3; }
source ./lib_battery.sh
declare -F crlf_guard >/dev/null || { echo "ABORT-LIB lib_battery.sh did not load"; exit 3; }
crlf_guard lib.sh lib_battery.sh verify4_run_all.sh verify4_battery.sh verify4_parse.py \
    stage3_parse.py tun_bulk.sh crownspot8.sh tail_matrix.sh perf_rwm_c.sh topo.sh \
    topo_dual.sh l1common.py transfer_bench.py
ROOT="${V4_ROOT:?V4_ROOT}"
: "${V4_HARD_DEADLINE:?V4_HARD_DEADLINE (epoch seconds) is required}"
: "${V4_OLD_SHA:?V4_OLD_SHA (the Stage-3 binary sha256) is required}"
OLD_REUSE="${V4_OLD_REUSE:-/home/vibe/v3/new/raptorpath}"
RUN="$ROOT/run"
SRC="$(cd "$HERE/../../.." && pwd)"
BNEW="$ROOT/bin/new/raptorpath"
BOLD="$ROOT/bin/old/raptorpath"
SOFT=$(( V4_HARD_DEADLINE - 600 ))
R_PRIOR=360          # s per (rep, seed) block of 19 invocations (§6 budget)
C_PRED=91            # s, the smoke plan's predicted summed invocation wall
SMOKE_PLAN="c2-100:NEW c8-100:OLD c3-25:NEWa c2-100:OLDa c1s-400:EMB"
D_RES=1500; CROWN_RES=2100
LAUNCH_ISO=$(date -u +%FT%TZ)
mkdir -p "$RUN" "$RUN/crown" "$ROOT/bin/new" "$ROOT/bin/old" 2>/dev/null

probe_sentinel "$RUN/all.out"
exec > >(tee -a "$RUN/all.out") 2>&1
SENTINELS="DONE-ALL FAILED-ALL FAILED-ALL-TRUNCATED-5H-BUDGET TRUNCATED.txt all-era.txt \
TESTS.txt TESTS-OK TESTS-FAIL SMOKE-PASS ABORT-SMOKE GO NOGO PLAN.txt BINSHA.txt build.log \
smoke.log smoke-tun.log smoke-crown.log smoke-check.txt v4.log tun.log score.txt"
# shellcheck disable=SC2086
LB_PROOF_EXTRA="launch=$LAUNCH_ISO hard=$V4_HARD_DEADLINE" prove_sentinels "$RUN" $SENTINELS
if ! sudo -n true 2>/dev/null; then
  echo "ABORT-SUDO sudo -n is not available to $(id -un); NOTHING WAS RUN."
  exit 3
fi
for f in $SENTINELS; do rm -f "$RUN/$f"; done
FIN="$RUN/.v4-finished"
STAGE_PIDFILE="$RUN/.v4-stage.pid"
rm -f "$FIN" "$STAGE_PIDFILE"
era() { echo "$* $(date -u +%FT%TZ)" | tee -a "$RUN/all-era.txt"; }
fail_all() { era "V4-ALL FAILED: $*"; touch "$RUN/FAILED-ALL"; exit 5; }

# ── LOCKS, for the whole session ─────────────────────────────────────────
LB_TAG="verify4_run_all:$$"
LB_LOG="$RUN/all-era.txt"
echo "V4-ALL start $LAUNCH_ISO load=$(cat /proc/loadavg) hard=$V4_HARD_DEADLINE soft=$SOFT" > "$RUN/all-era.txt"
install_lock_traps
VM_LOCK="${RWM_VM_LOCK:-/tmp/rwm-vm.lock}"
RP_LOCK="${RWM_RP_LOCK:-/home/vibe/rp.lock}"
take_lock "$VM_LOCK"
take_lock "$RP_LOCK"
export V4_LOCK_OWNER="verify4_run_all:$$"
refresh_locks() { # stage-name
  local l
  for l in "$VM_LOCK" "$RP_LOCK"; do
    if ! grep -qF -- "$V4_LOCK_OWNER" "$l" 2>/dev/null; then
      era "LOCK-TRUNCATED-BY-FOREIGN $l before $1: '$(cat "$l" 2>/dev/null)'"
    fi
    echo "$$ $LB_TAG $(date -u +%FT%TZ)" > "$l"
  done
}
cotenants() { echo "cargo=$(pgrep -xc cargo) rustc=$(pgrep -xc rustc) load=$(cut -d' ' -f1-3 /proc/loadavg)"; }

backstop() {
  while :; do
    [ -f "$FIN" ] && exit 0
    [ "$(date +%s)" -ge "$V4_HARD_DEADLINE" ] && break
    sleep 30
  done
  local now; now=$(date -u +%FT%TZ)
  echo "V4-ALL BACKSTOP fired $now"
  { echo "TRUNCATED by verify4_run_all.sh backstop at $now"
    echo "launched $LAUNCH_ISO; hard deadline $V4_HARD_DEADLINE; the 5 h cap, not a battery failure"; } > "$RUN/TRUNCATED.txt"
  local spid; spid=$(cat "$STAGE_PIDFILE" 2>/dev/null)
  [ -n "$spid" ] && sudo -n kill -TERM "$spid" 2>/dev/null
  [ -n "$spid" ] && kill -TERM "$spid" 2>/dev/null
  sudo -n pkill -x raptorpath 2>/dev/null || true
  local i
  for i in $(seq 1 36); do
    [ "$(sudo -n ip netns list 2>/dev/null | grep -c '^rp-')" -eq 0 ] && break
    sleep 5
  done
  touch "$RUN/FAILED-ALL-TRUNCATED-5H-BUDGET"
  echo "V4-ALL truncated $(date -u +%FT%TZ)" >> "$RUN/all-era.txt"
}
backstop &
BACKSTOP_PID=$!
finish() { touch "$FIN" 2>/dev/null; kill "$BACKSTOP_PID" 2>/dev/null; release_locks; }
trap finish EXIT
trap 'finish; exit 130' INT
trap 'finish; exit 143' TERM
stage() { # run "$@" as the tracked stage (the backstop TERMs it)
  "$@" &
  echo $! > "$STAGE_PIDFILE"; wait "$(cat "$STAGE_PIDFILE")"; local rc=$?; rm -f "$STAGE_PIDFILE"
  return $rc
}

# ── 1. BUILD + TESTS (NEW tree) ──────────────────────────────────────────
refresh_locks build
CARGO="$(command -v cargo || echo "$HOME/.cargo/bin/cargo")"
era "V4-ALL build start commit=$(cat "$SRC/COMMIT" 2>/dev/null) cargo=$CARGO cotenants: $(cotenants)"
T0=$(date +%s)
( cd "$SRC" && stage "$CARGO" build --release ) > "$RUN/build.log" 2>&1
BRC=$?
era "V4-ALL cargo build --release rc=$BRC wall=$(( $(date +%s) - T0 ))s"
[ "$BRC" = "0" ] || fail_all "ABORT-BUILD rc=$BRC (see build.log)"
: > "$RUN/TESTS.txt"
tst() { # name -- cmd...
  local name="$1"; shift
  local t0; t0=$(date +%s)
  ( cd "$SRC" && stage "$@" ) > "$RUN/test-$name.log" 2>&1
  local rc=$?
  local sum
  sum=$(grep -a '^test result:' "$RUN/test-$name.log" | awk '{p+=$4; f+=$6; i+=$8} END{printf "passed=%d failed=%d ignored=%d binaries=%d", p, f, i, NR}')
  echo "TEST $name rc=$rc wall=$(( $(date +%s) - t0 ))s $sum cmd=[$*]" >> "$RUN/TESTS.txt"
  era "V4-ALL test $name rc=$rc $sum"
  return $rc
}
TFAIL=0
tst main "$CARGO" test -p raptorpath -p raptorpath-math --release --no-fail-fast -- --test-threads=2 || TFAIL=1
tst doc "$CARGO" test --doc -p raptorpath --release || TFAIL=1
tst wasm env -u GOLDEN_CAPTURE "$CARGO" test -p raptorpath-wasm || TFAIL=1
# (c) the gate_harness determinism check: the fixed harness twice, then the
# pre-fix harness ($ROOT/prefix/gate_harness.rs) twice for contrast; the
# printed outcomes (timing lines stripped) must be identical within a pair.
gs() { # tag
  ( cd "$SRC" && stage "$CARGO" test --release -p raptorpath --test gate_suite -- --nocapture --test-threads=1 ) \
      > "$RUN/gate-$1.log" 2>&1
  local rc=$?
  grep -av -E 'finished in|Running |Finished |Compiling |^\s*$' "$RUN/gate-$1.log" > "$RUN/gate-$1.out"
  echo "GATE $1 rc=$rc lines=$(wc -l < "$RUN/gate-$1.out") md5=$(md5sum < "$RUN/gate-$1.out" | cut -c1-12)" >> "$RUN/TESTS.txt"
  return $rc
}
gs fixed1 || TFAIL=1
gs fixed2 || TFAIL=1
if cmp -s "$RUN/gate-fixed1.out" "$RUN/gate-fixed2.out"; then
  echo "GATE-DETERMINISM fixed IDENTICAL" >> "$RUN/TESTS.txt"
else
  echo "GATE-DETERMINISM fixed DIFFER ($(diff "$RUN/gate-fixed1.out" "$RUN/gate-fixed2.out" | grep -c '^[<>]') lines)" >> "$RUN/TESTS.txt"
fi
if [ -f "$ROOT/prefix/gate_harness.rs" ]; then
  H="$SRC/raptorpath/tests/common/gate_harness.rs"
  cp "$H" "$ROOT/prefix/gate_harness.fixed.rs"
  cp "$ROOT/prefix/gate_harness.rs" "$H"
  gs prefix1; gs prefix2
  cp "$ROOT/prefix/gate_harness.fixed.rs" "$H"
  if cmp -s "$RUN/gate-prefix1.out" "$RUN/gate-prefix2.out"; then
    echo "GATE-DETERMINISM prefix IDENTICAL" >> "$RUN/TESTS.txt"
  else
    echo "GATE-DETERMINISM prefix DIFFER ($(diff "$RUN/gate-prefix1.out" "$RUN/gate-prefix2.out" | grep -c '^[<>]') lines)" >> "$RUN/TESTS.txt"
  fi
fi
( cd "$SRC" && stage "$CARGO" build --release --bin raptorpath ) >> "$RUN/build.log" 2>&1 \
  || fail_all "ABORT-BUILD --bin raptorpath"
cp "$SRC/target/release/raptorpath" "$BNEW" || fail_all "ABORT-BUILD copy"
V4_SHA_NEW="$(sha256sum "$BNEW" | cut -d' ' -f1)"
if [ "$TFAIL" = "0" ]; then touch "$RUN/TESTS-OK"; else touch "$RUN/TESTS-FAIL"; fi
era "V4-ALL tests done tfail=$TFAIL new sha256=$V4_SHA_NEW"

# ── 2. OLD binary ────────────────────────────────────────────────────────
if [ -f "$OLD_REUSE" ] && [ "$(sha256sum "$OLD_REUSE" | cut -d' ' -f1)" = "$V4_OLD_SHA" ]; then
  cp "$OLD_REUSE" "$BOLD"
  OLD_HOW="reused $OLD_REUSE"
else
  [ -d "$ROOT/old-src" ] || fail_all "ABORT-OLD-BINARY no reusable binary and no $ROOT/old-src"
  ( cd "$ROOT/old-src" && stage "$CARGO" build --release --bin raptorpath ) > "$RUN/build-old.log" 2>&1 \
    || fail_all "ABORT-OLD-BINARY build failed"
  cp "$ROOT/old-src/target/release/raptorpath" "$BOLD"
  OLD_HOW="built from $ROOT/old-src ($(cat "$ROOT/old-src/COMMIT" 2>/dev/null))"
fi
V4_SHA_OLD="$(sha256sum "$BOLD" | cut -d' ' -f1)"
[ "$V4_SHA_OLD" = "$V4_OLD_SHA" ] || fail_all "ABORT-OLD-BINARY sha $V4_SHA_OLD != $V4_OLD_SHA ($OLD_HOW)"
{ echo "$V4_SHA_NEW  $BNEW  commit=$(cat "$SRC/COMMIT" 2>/dev/null) built=$(date -u +%FT%TZ)"
  echo "$V4_SHA_OLD  $BOLD  $OLD_HOW"; } > "$RUN/BINSHA.txt"
era "V4-ALL binaries new=$V4_SHA_NEW old=$V4_SHA_OLD ($OLD_HOW)"
export V4_SHA_NEW V4_SHA_OLD

# ── 3. SMOKE ─────────────────────────────────────────────────────────────
refresh_locks smoke
T0=$(date +%s)
stage sudo -n env V4_SHA_NEW="$V4_SHA_NEW" V4_SHA_OLD="$V4_SHA_OLD" V4_LOCK_OWNER="$V4_LOCK_OWNER" \
    V4_OUTDIR="$RUN" V4_TAG=smoke V4_SEEDS=42 V4_SMOKE_PLAN="$SMOKE_PLAN" \
    V4_BIN_NEW="$BNEW" V4_BIN_OLD="$BOLD" bash ./verify4_battery.sh 1
SRC_RC=$?
{ echo "=== TUNBIN new $V4_SHA_NEW"; echo "=== TUNBIN old $V4_SHA_OLD"; } > "$RUN/smoke-tun.log"
stage sudo -n env SEED=42 RWM_GEN=0 RWM_BIN="$BNEW" bash ./tun_bulk.sh c2 bulk 5000000 1 new-bulk >> "$RUN/smoke-tun.log" 2>&1
stage sudo -n env SEED=42 RWM_GEN=0 RWM_BIN="$BOLD" bash ./tun_bulk.sh c2 bulk 5000000 1 old-bulk >> "$RUN/smoke-tun.log" 2>&1
stage sudo -n env RWM_GEN=0 RWM_DIAG=1 RWM_TM_ARMS=ship SEED=42 RWM_TM_SIZES=400 RWM_BIN="$BNEW" \
    bash ./tail_matrix.sh c2 1 > "$RUN/smoke-crown.log" 2>&1
era "V4-ALL smoke rc=$SRC_RC wall=$(( $(date +%s) - T0 ))s cotenants: $(cotenants)"
if [ "$SRC_RC" != "0" ] || ! python3 ./verify4_parse.py smoke "$RUN/smoke.log" "$RUN/smoke-tun.log" \
      "$RUN/smoke-crown.log" > "$RUN/smoke-check.txt" 2>&1; then
  cat "$RUN/smoke-check.txt" 2>/dev/null
  touch "$RUN/ABORT-SMOKE"
  fail_all "ABORT-SMOKE (smoke-check.txt)"
fi
cat "$RUN/smoke-check.txt"
C_MEAS=$(python3 ./verify4_parse.py cost "$RUN/smoke.log" | sed -n 's/^COST total=\([0-9]*\) .*/\1/p')
C_MEAS="${C_MEAS:-0}"
R_EST=$(( R_PRIOR * C_MEAS / C_PRED ))
[ "$R_EST" -lt "$R_PRIOR" ] && R_EST=$R_PRIOR
era "V4-ALL SMOKE-PASS c_meas=${C_MEAS}s c_pred=${C_PRED}s R_est=${R_EST}s"
touch "$RUN/SMOKE-PASS"

# ── GO gate (operator reads TESTS.txt and the smoke, not a battery) ──────
GO_DEADLINE=$(( $(date +%s) + 2400 ))
while :; do
  [ -f "$RUN/GO" ] && break
  [ -f "$RUN/NOGO" ] && fail_all "NOGO by the operator after the tests/smoke"
  [ "$(date +%s)" -ge "$GO_DEADLINE" ] && fail_all "NO-GO-TIMEOUT (40 min after SMOKE-PASS)"
  sleep 10
done
era "V4-ALL GO received"

# ── 4. BUDGET ────────────────────────────────────────────────────────────
CROWN_REPS=8; NO_D=0; NO_EMB=0; NO_AUTO=0; R_USE=$R_EST
plan_n() { local avail=$(( SOFT - $(date +%s) - CROWN_RES - D_RES )); echo $(( avail > 0 ? avail / (2 * R_USE) : 0 )); }
capn() { N=$(plan_n); [ "$N" -gt 8 ] && N=8; }
capn
CUTS=""
if [ "$N" -lt 4 ]; then CROWN_REPS=6; CROWN_RES=1620; capn; CUTS="$CUTS crown-reps-6"; fi
if [ "$N" -lt 4 ]; then CROWN_REPS=0; CROWN_RES=0; capn; CUTS="$CUTS drop-crown"; fi
if [ "$N" -lt 4 ]; then NO_D=1; D_RES=0; capn; CUTS="$CUTS drop-tunnel"; fi
if [ "$N" -lt 4 ]; then NO_EMB=1; R_USE=$(( R_EST * 16 / 19 )); capn; CUTS="$CUTS drop-EMB"; fi
if [ "$N" -lt 4 ]; then NO_AUTO=1; R_USE=$(( R_EST * 12 / 19 )); capn; CUTS="$CUTS drop-auto"; fi
[ "$N" -lt 3 ] && fail_all "ABORT-BUDGET n=$N after cuts:$CUTS"
BSOFT=$(( SOFT - CROWN_RES - D_RES ))
{ echo "n_per_seed=$N seeds=42,7 block_est=${R_USE}s crown_reps=$CROWN_REPS crown_reserve=${CROWN_RES}s tunnel=$((1 - NO_D)) tunnel_reserve=${D_RES}s no_emb=$NO_EMB no_auto=$NO_AUTO cuts='${CUTS:- none}'"
  echo "battery_soft_deadline=$BSOFT soft=$SOFT hard=$V4_HARD_DEADLINE now=$(date +%s) c_meas=$C_MEAS c_pred=$C_PRED r_prior=$R_PRIOR"; } > "$RUN/PLAN.txt"
era "V4-ALL PLAN $(head -1 "$RUN/PLAN.txt")"

# ── 5. BATTERY ───────────────────────────────────────────────────────────
refresh_locks battery
T0=$(date +%s)
era "V4-ALL battery start cotenants: $(cotenants)"
stage sudo -n env V4_SHA_NEW="$V4_SHA_NEW" V4_SHA_OLD="$V4_SHA_OLD" V4_LOCK_OWNER="$V4_LOCK_OWNER" \
    V4_OUTDIR="$RUN" V4_TAG=v4 V4_SEEDS="42 7" V4_NO_EMB="$NO_EMB" V4_NO_AUTO="$NO_AUTO" \
    V4_SOFT_DEADLINE="$BSOFT" V4_BLOCK_EST_S="$R_USE" V4_BIN_NEW="$BNEW" V4_BIN_OLD="$BOLD" \
    bash ./verify4_battery.sh "$N"
BRC=$?
era "V4-ALL battery rc=$BRC wall=$(( $(date +%s) - T0 ))s cotenants: $(cotenants)"
BAT_OK=0
if [ "$BRC" = "0" ] && [ -s "$RUN/v4.log" ] && grep -aq "V4-BATTERY-DONE" "$RUN/v4.log" \
    && python3 ./verify4_parse.py check "$RUN/v4.log"; then
  BAT_OK=1
fi
[ -f "$RUN/TRUNCATED.txt" ] && exit 6

# ── 6. TUNNEL (D) ────────────────────────────────────────────────────────
# 2 rounds x seeds 42, 7 x cells c2, c3 x arms {new,old} x {bulk,auto},
# arm order rotated per (round, seed, cell); 4 cold TCP transfers per
# bring-up; c2 50 MB, c3 12 MB.
D_STATE="cut-by-plan"
if [ "$NO_D" = "0" ]; then
  if [ $(( $(date +%s) + D_RES )) -gt $(( SOFT - CROWN_RES )) ]; then
    D_STATE="skipped-5h-budget"
  else
    refresh_locks tunnel
    T0=$(date +%s)
    { echo "=== TUNBIN new $V4_SHA_NEW"; echo "=== TUNBIN old $V4_SHA_OLD"; } > "$RUN/tun.log"
    K=0
    for ROUND in 1 2; do
      for SEED in 42 7; do
        for CELL in c2 c3; do
          case $CELL in c2) BYTES=50000000 ;; c3) BYTES=12000000 ;; esac
          ARMS=(new-bulk old-bulk new-auto old-auto)
          for ((j = 0; j < 4; j++)); do
            A="${ARMS[$(( (j + K) % 4 ))]}"
            B="$BNEW"; [ "${A%%-*}" = "old" ] && B="$BOLD"
            echo "=== TUN round=$ROUND seed=$SEED cell=$CELL arm=$A $(date -u +%T)" >> "$RUN/tun.log"
            stage sudo -n env SEED="$SEED" RWM_GEN=0 RWM_BIN="$B" \
                bash ./tun_bulk.sh "$CELL" "${A##*-}" "$BYTES" 4 "$A" >> "$RUN/tun.log" 2>&1
          done
          K=$(( K + 1 ))
        done
      done
    done
    echo "TUN-DONE $(date -u +%FT%TZ)" >> "$RUN/tun.log"
    D_STATE=done
    era "V4-ALL tunnel done wall=$(( $(date +%s) - T0 ))s"
  fi
fi
[ -f "$RUN/TRUNCATED.txt" ] && exit 6

# ── 7. CROWN (E) ─────────────────────────────────────────────────────────
CROWN_STATE="cut-by-plan"
if [ "$CROWN_REPS" -gt 0 ]; then
  if [ $(( $(date +%s) + CROWN_RES )) -gt "$SOFT" ]; then
    CROWN_STATE="skipped-5h-budget"
  else
    refresh_locks crown
    T0=$(date +%s)
    stage env CROWNSPOT_OUT="$RUN/crown" CROWNSPOT_REPS="$CROWN_REPS" RWM_BIN="$BNEW" \
      bash ./crownspot8.sh > "$RUN/crown/run.out" 2>&1
    CROWN_STATE=$([ -f "$RUN/crown/DONE-ALL" ] && echo done || echo failed)
    era "V4-ALL crown $CROWN_STATE wall=$(( $(date +%s) - T0 ))s cotenants: $(cotenants)"
  fi
fi
[ -f "$RUN/TRUNCATED.txt" ] && exit 6

# ── 8. SCORE + SENTINEL ─────────────────────────────────────────────────
{ python3 ./verify4_parse.py score "$RUN/v4.log"
  echo
  [ -s "$RUN/tun.log" ] && python3 ./verify4_parse.py tun "$RUN/tun.log"
  echo
  [ -f "$RUN/crown/crown-s42.log" ] && python3 ./stage3_parse.py crown "$RUN/crown/crown-s42.log" "$RUN/crown/crown-s7.log"
} > "$RUN/score.txt" 2>&1
SOFT_TRUNC=0
grep -aq "TRUNCATED-AT-REP-BOUNDARY" "$RUN/v4.log" 2>/dev/null && SOFT_TRUNC=1
era "V4-ALL end bat_ok=$BAT_OK soft_trunc=$SOFT_TRUNC tunnel=$D_STATE crown=$CROWN_STATE load=$(cut -d' ' -f1-3 /proc/loadavg)"
if [ "$BAT_OK" -eq 1 ] && { [ "$SOFT_TRUNC" -eq 1 ] || [ "$CROWN_STATE" = "skipped-5h-budget" ] || [ "$D_STATE" = "skipped-5h-budget" ]; }; then
  touch "$RUN/FAILED-ALL-TRUNCATED-5H-BUDGET"; exit 6
fi
if [ "$BAT_OK" -eq 1 ] && [ "$D_STATE" != "skipped-5h-budget" ] \
    && { [ "$CROWN_STATE" = "done" ] || [ "$CROWN_STATE" = "cut-by-plan" ]; }; then
  touch "$RUN/DONE-ALL"; echo "V4-ALL-DONE"
  exit 0
fi
fail_all "battery_ok=$BAT_OK tunnel=$D_STATE crown=$CROWN_STATE"
