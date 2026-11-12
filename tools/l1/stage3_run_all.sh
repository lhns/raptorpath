#!/bin/bash
# THE STAGE-3 BASELINE'S 5 h ENVELOPE (docs/status.md §5 pre-registration;
# docs/measurement-discipline.md "The five-hour cap").
#
#   S3_ROOT=/home/vibe/stage3 S3_HARD_DEADLINE=<epoch> \
#     setsid nohup bash stage3_run_all.sh > $S3_ROOT/launch.out 2>&1 < /dev/null &
#
# Run from the synced tree ($S3_ROOT/src/raptorpath/tools/l1). STARTED AS
# `vibe`, NOT ROOT: sentinels are proven writable and written by vibe; `sudo
# -n` is used for the measurement stages only.
#
# ORDER (pre-registered), all under both operator locks taken HERE:
#   1. build: `cargo build --release --bin raptorpath` in $S3_ROOT/src, copy
#      to $S3_ROOT/bin/raptorpath (its real name), sha256 -> BINSHA.txt;
#   2. smoke: one invocation per arm (stage3_battery.sh, S3_SMOKE_PLAN) plus
#      one tail_matrix `ship` rep; `stage3_parse.py smoke` must pass, else
#      ABORT-SMOKE and nothing else runs. SMOKE-PASS is written, then the
#      envelope waits (<= 20 min) for the operator's GO (NOGO or timeout:
#      FAILED-ALL, nothing else runs);
#   3. the budget rule (the cut order below) fixes n per seed, crown reps and
#      the PACK arm (the per-ack control, RWM_EST_CADENCE=0; section 5 ran
#      it as CAD = 1 before the cadence became the default) -> PLAN.txt;
#   4. the battery (stage3_battery.sh), soft-truncated at (rep, seed)
#      boundaries so the crown's reserve survives;
#   5. the crown spot (crownspot8.sh) if its reserve is intact;
#   6. score -> score.txt (stage3_parse.py score; stage3_parse.py crown).
#
# BUDGET RULE (pre-registered): R_est = R_PRIOR * max(1, c_meas / C_PRED),
# where c_meas is the smoke battery's summed invocation wall and C_PRED its
# predicted sum. avail = SOFT - now - crown_reserve; n = min(5,
# floor(avail / (2 * R_est))). Cut order while n < 3: crown reps 8 -> 6;
# drop PACK (R_est * 27/32); drop the crown. n < 2 after every cut:
# ABORT-BUDGET (nothing runs).
#
# SENTINELS: DONE-ALL only when the battery ledger carries
# STAGE3-BATTERY-DONE, `stage3_parse.py check` returns 0, no truncation
# happened, and the crown earned its DONE-ALL (or was cut by the budget rule).
# FAILED-ALL (cause in all-era.txt), FAILED-ALL-TRUNCATED-5H-BUDGET (soft or
# hard truncation; score what is balanced). The HARD backstop at
# S3_HARD_DEADLINE TERMs the stage, `pkill -x raptorpath`, waits for the rp-*
# namespaces to clear, and writes FAILED-ALL-TRUNCATED-5H-BUDGET.
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE" || { echo "ABORT-CD $HERE"; exit 3; }
source ./lib_battery.sh
declare -F crlf_guard >/dev/null || { echo "ABORT-LIB lib_battery.sh did not load"; exit 3; }
crlf_guard lib.sh lib_battery.sh stage3_run_all.sh stage3_battery.sh stage3_parse.py \
    crownspot8.sh tail_matrix.sh perf_rwm_c.sh topo.sh topo_dual.sh l1common.py
ROOT="${S3_ROOT:?S3_ROOT}"
: "${S3_HARD_DEADLINE:?S3_HARD_DEADLINE (epoch seconds) is required}"
RUN="$ROOT/run"
SRC="$(cd "$HERE/../../.." && pwd)"
BIN="$ROOT/bin/raptorpath"
SOFT=$(( S3_HARD_DEADLINE - 600 ))
R_PRIOR=1200
C_PRED=128
SMOKE_PLAN="c2-100:A1 c7-100:A2 c1d-400:PACK c7-100:WINa"
LAUNCH_ISO=$(date -u +%FT%TZ)
mkdir -p "$RUN" "$RUN/crown" "$ROOT/bin" 2>/dev/null

probe_sentinel "$RUN/all.out"
exec > >(tee -a "$RUN/all.out") 2>&1
SENTINELS="DONE-ALL FAILED-ALL FAILED-ALL-TRUNCATED-5H-BUDGET TRUNCATED.txt all-era.txt \
SMOKE-PASS ABORT-SMOKE GO NOGO PLAN.txt BINSHA.txt build.log smoke.log smoke-crown.log \
s3.log score.txt"
# shellcheck disable=SC2086
LB_PROOF_EXTRA="launch=$LAUNCH_ISO hard=$S3_HARD_DEADLINE" prove_sentinels "$RUN" $SENTINELS
if ! sudo -n true 2>/dev/null; then
  echo "ABORT-SUDO sudo -n is not available to $(id -un); NOTHING WAS RUN."
  exit 3
fi
for f in $SENTINELS; do rm -f "$RUN/$f"; done
FIN="$RUN/.s3-finished"
STAGE_PIDFILE="$RUN/.s3-stage.pid"
rm -f "$FIN" "$STAGE_PIDFILE"
era() { echo "$* $(date -u +%FT%TZ)" | tee -a "$RUN/all-era.txt"; }
fail_all() { era "S3-ALL FAILED: $*"; touch "$RUN/FAILED-ALL"; exit 5; }

# ── LOCKS, for the whole session ─────────────────────────────────────────
LB_TAG="stage3_run_all:$$"
LB_LOG="$RUN/all-era.txt"
echo "S3-ALL start $LAUNCH_ISO load=$(cat /proc/loadavg) hard=$S3_HARD_DEADLINE soft=$SOFT" > "$RUN/all-era.txt"
install_lock_traps
VM_LOCK="${RWM_VM_LOCK:-/tmp/rwm-vm.lock}"
RP_LOCK="${RWM_RP_LOCK:-/home/vibe/rp.lock}"
take_lock "$VM_LOCK"
take_lock "$RP_LOCK"
export S3_LOCK_OWNER="stage3_run_all:$$"
refresh_locks() { # stage-name
  local l
  for l in "$VM_LOCK" "$RP_LOCK"; do
    if ! grep -qF -- "$S3_LOCK_OWNER" "$l" 2>/dev/null; then
      era "LOCK-TRUNCATED-BY-FOREIGN $l before $1: '$(cat "$l" 2>/dev/null)'"
    fi
    echo "$$ $LB_TAG $(date -u +%FT%TZ)" > "$l"
  done
}
cotenants() { echo "cargo=$(pgrep -xc cargo) rustc=$(pgrep -xc rustc) load=$(cut -d' ' -f1-3 /proc/loadavg)"; }

backstop() {
  while :; do
    [ -f "$FIN" ] && exit 0
    [ "$(date +%s)" -ge "$S3_HARD_DEADLINE" ] && break
    sleep 30
  done
  local now; now=$(date -u +%FT%TZ)
  echo "S3-ALL BACKSTOP fired $now"
  { echo "TRUNCATED by stage3_run_all.sh backstop at $now"
    echo "launched $LAUNCH_ISO; hard deadline $S3_HARD_DEADLINE; the 5 h cap, not a battery failure"; } > "$RUN/TRUNCATED.txt"
  local spid; spid=$(cat "$STAGE_PIDFILE" 2>/dev/null)
  [ -n "$spid" ] && sudo -n kill -TERM "$spid" 2>/dev/null
  sudo -n pkill -x raptorpath 2>/dev/null || true
  local i
  for i in $(seq 1 36); do
    [ "$(sudo -n ip netns list 2>/dev/null | grep -c '^rp-')" -eq 0 ] && break
    sleep 5
  done
  touch "$RUN/FAILED-ALL-TRUNCATED-5H-BUDGET"
  echo "S3-ALL truncated $(date -u +%FT%TZ)" >> "$RUN/all-era.txt"
}
backstop &
BACKSTOP_PID=$!
finish() { touch "$FIN" 2>/dev/null; kill "$BACKSTOP_PID" 2>/dev/null; release_locks; }
trap finish EXIT
trap 'finish; exit 130' INT
trap 'finish; exit 143' TERM

# ── 1. BUILD ─────────────────────────────────────────────────────────────
refresh_locks build
CARGO="$(command -v cargo || echo "$HOME/.cargo/bin/cargo")"
era "S3-ALL build start commit=$(cat "$SRC/COMMIT" 2>/dev/null) cargo=$CARGO cotenants: $(cotenants)"
( cd "$SRC" && "$CARGO" build --release --bin raptorpath ) > "$RUN/build.log" 2>&1
BRC=$?
[ "$BRC" = "0" ] || fail_all "ABORT-BUILD rc=$BRC (see build.log)"
cp "$SRC/target/release/raptorpath" "$BIN" || fail_all "ABORT-BUILD copy"
S3_SHA="$(sha256sum "$BIN" | cut -d' ' -f1)"
echo "$S3_SHA  $BIN  commit=$(cat "$SRC/COMMIT" 2>/dev/null) built=$(date -u +%FT%TZ)" > "$RUN/BINSHA.txt"
era "S3-ALL build done sha256=$S3_SHA"
export S3_SHA

# ── 2. SMOKE ─────────────────────────────────────────────────────────────
refresh_locks smoke
T0=$(date +%s)
sudo -n env S3_SHA="$S3_SHA" S3_LOCK_OWNER="$S3_LOCK_OWNER" S3_OUTDIR="$RUN" S3_TAG=smoke \
    S3_SEEDS=42 S3_SMOKE_PLAN="$SMOKE_PLAN" RWM_BIN="$BIN" \
    bash ./stage3_battery.sh 1 &
echo $! > "$STAGE_PIDFILE"; wait "$(cat "$STAGE_PIDFILE")"; SRC_RC=$?; rm -f "$STAGE_PIDFILE"
sudo -n env RWM_GEN=0 RWM_DIAG=1 RWM_TM_ARMS=ship SEED=42 RWM_TM_SIZES=400 RWM_BIN="$BIN" \
    bash ./tail_matrix.sh c2 1 > "$RUN/smoke-crown.log" 2>&1 &
echo $! > "$STAGE_PIDFILE"; wait "$(cat "$STAGE_PIDFILE")"; rm -f "$STAGE_PIDFILE"
era "S3-ALL smoke rc=$SRC_RC wall=$(( $(date +%s) - T0 ))s cotenants: $(cotenants)"
if [ "$SRC_RC" != "0" ] || ! python3 ./stage3_parse.py smoke "$RUN/smoke.log" "$RUN/smoke-crown.log" > "$RUN/smoke-check.txt" 2>&1; then
  cat "$RUN/smoke-check.txt" 2>/dev/null
  touch "$RUN/ABORT-SMOKE"
  fail_all "ABORT-SMOKE (smoke-check.txt)"
fi
cat "$RUN/smoke-check.txt"
C_MEAS=$(python3 ./stage3_parse.py cost "$RUN/smoke.log" | sed -n 's/^COST total=\([0-9]*\) .*/\1/p')
C_MEAS="${C_MEAS:-0}"
R_EST=$(( R_PRIOR * C_MEAS / C_PRED ))
[ "$R_EST" -lt "$R_PRIOR" ] && R_EST=$R_PRIOR
era "S3-ALL SMOKE-PASS c_meas=${C_MEAS}s c_pred=${C_PRED}s R_est=${R_EST}s"
touch "$RUN/SMOKE-PASS"

# ── GO gate (operator reads the smoke, not a battery) ────────────────────
GO_DEADLINE=$(( $(date +%s) + 1200 ))
while :; do
  [ -f "$RUN/GO" ] && break
  [ -f "$RUN/NOGO" ] && fail_all "NOGO by the operator after the smoke"
  [ "$(date +%s)" -ge "$GO_DEADLINE" ] && fail_all "NO-GO-TIMEOUT (20 min after SMOKE-PASS)"
  sleep 10
done
era "S3-ALL GO received"

# ── 3. BUDGET ────────────────────────────────────────────────────────────
CROWN_REPS=8; CROWN_RES=2100; NO_CAD=0; R_USE=$R_EST
plan_n() { local avail=$(( SOFT - $(date +%s) - CROWN_RES )); echo $(( avail > 0 ? avail / (2 * R_USE) : 0 )); }
N=$(plan_n); [ "$N" -gt 5 ] && N=5
CUTS=""
if [ "$N" -lt 3 ]; then CROWN_REPS=6; CROWN_RES=1620; N=$(plan_n); [ "$N" -gt 5 ] && N=5; CUTS="$CUTS crown-reps-6"; fi
if [ "$N" -lt 3 ]; then NO_CAD=1; R_USE=$(( R_EST * 27 / 32 )); N=$(plan_n); [ "$N" -gt 5 ] && N=5; CUTS="$CUTS drop-PACK"; fi
if [ "$N" -lt 3 ]; then CROWN_REPS=0; CROWN_RES=0; N=$(plan_n); [ "$N" -gt 5 ] && N=5; CUTS="$CUTS drop-crown"; fi
[ "$N" -lt 2 ] && fail_all "ABORT-BUDGET n=$N after cuts:$CUTS"
BSOFT=$(( SOFT - CROWN_RES ))
{ echo "n_per_seed=$N seeds=42,7 block_est=${R_USE}s crown_reps=$CROWN_REPS crown_reserve=${CROWN_RES}s no_cad=$NO_CAD cuts='${CUTS:- none}'"
  echo "battery_soft_deadline=$BSOFT soft=$SOFT hard=$S3_HARD_DEADLINE now=$(date +%s) c_meas=$C_MEAS c_pred=$C_PRED r_prior=$R_PRIOR"; } > "$RUN/PLAN.txt"
era "S3-ALL PLAN $(head -1 "$RUN/PLAN.txt")"

# ── 4. BATTERY ───────────────────────────────────────────────────────────
refresh_locks battery
T0=$(date +%s)
era "S3-ALL battery start cotenants: $(cotenants)"
sudo -n env S3_SHA="$S3_SHA" S3_LOCK_OWNER="$S3_LOCK_OWNER" S3_OUTDIR="$RUN" S3_TAG=s3 \
    S3_SEEDS="42 7" S3_NO_PACK="$NO_CAD" S3_SOFT_DEADLINE="$BSOFT" S3_BLOCK_EST_S="$R_USE" RWM_BIN="$BIN" \
    bash ./stage3_battery.sh "$N" &
echo $! > "$STAGE_PIDFILE"; wait "$(cat "$STAGE_PIDFILE")"; BRC=$?; rm -f "$STAGE_PIDFILE"
era "S3-ALL battery rc=$BRC wall=$(( $(date +%s) - T0 ))s cotenants: $(cotenants)"
BAT_OK=0
if [ "$BRC" = "0" ] && [ -s "$RUN/s3.log" ] && grep -aq "STAGE3-BATTERY-DONE" "$RUN/s3.log" \
    && python3 ./stage3_parse.py check "$RUN/s3.log"; then
  BAT_OK=1
fi
[ -f "$RUN/TRUNCATED.txt" ] && exit 6

# ── 5. CROWN ─────────────────────────────────────────────────────────────
CROWN_STATE="cut-by-plan"
if [ "$CROWN_REPS" -gt 0 ]; then
  if [ $(( $(date +%s) + CROWN_RES )) -gt "$SOFT" ]; then
    CROWN_STATE="skipped-5h-budget"
  else
    refresh_locks crown
    T0=$(date +%s)
    CROWNSPOT_OUT="$RUN/crown" CROWNSPOT_REPS="$CROWN_REPS" RWM_BIN="$BIN" \
      bash ./crownspot8.sh > "$RUN/crown/run.out" 2>&1 &
    echo $! > "$STAGE_PIDFILE"; wait "$(cat "$STAGE_PIDFILE")"; rm -f "$STAGE_PIDFILE"
    CROWN_STATE=$([ -f "$RUN/crown/DONE-ALL" ] && echo done || echo failed)
    era "S3-ALL crown $CROWN_STATE wall=$(( $(date +%s) - T0 ))s cotenants: $(cotenants)"
  fi
fi
[ -f "$RUN/TRUNCATED.txt" ] && exit 6

# ── 6. SCORE + SENTINEL ─────────────────────────────────────────────────
{ python3 ./stage3_parse.py score "$RUN/s3.log"
  echo
  [ -f "$RUN/crown/crown-s42.log" ] && python3 ./stage3_parse.py crown "$RUN/crown/crown-s42.log" "$RUN/crown/crown-s7.log"
} > "$RUN/score.txt" 2>&1
SOFT_TRUNC=0
grep -aq "TRUNCATED-AT-REP-BOUNDARY" "$RUN/s3.log" 2>/dev/null && SOFT_TRUNC=1
era "S3-ALL end bat_ok=$BAT_OK soft_trunc=$SOFT_TRUNC crown=$CROWN_STATE load=$(cut -d' ' -f1-3 /proc/loadavg)"
if [ "$BAT_OK" -eq 1 ] && { [ "$SOFT_TRUNC" -eq 1 ] || [ "$CROWN_STATE" = "skipped-5h-budget" ]; }; then
  touch "$RUN/FAILED-ALL-TRUNCATED-5H-BUDGET"; exit 6
fi
if [ "$BAT_OK" -eq 1 ] && { [ "$CROWN_STATE" = "done" ] || [ "$CROWN_STATE" = "cut-by-plan" ]; }; then
  touch "$RUN/DONE-ALL"; echo "S3-ALL-DONE"
  exit 0
fi
fail_all "battery_ok=$BAT_OK crown=$CROWN_STATE"
