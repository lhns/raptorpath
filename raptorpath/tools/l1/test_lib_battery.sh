#!/bin/bash
# Offline tests for lib_battery.sh (no root, no engine, no namespaces).
#   bash test_lib_battery.sh      # exit 0 iff every check passes
set -uo pipefail
cd "$(dirname "$0")" || exit 2
TD="$(mktemp -d)"
trap 'rm -rf "$TD"' EXIT
FAIL=0
ok()   { printf 'ok    %s\n' "$1"; }
bad()  { printf 'FAIL  %s\n' "$1"; FAIL=1; }
ckeq() { if [ "$2" = "$3" ]; then ok "$1 ($3)"; else bad "$1: want '$2' got '$3'"; fi; }

# Sourcing must not turn on errexit (the batteries run with per-arm tolerance).
( set +e; source ./lib_battery.sh; case $- in *e*) exit 1 ;; esac; exit 0 ) \
  && ok "sourcing does not set -e" || bad "sourcing set -e"
source ./lib_battery.sh

# ── field: token-anchored, first occurrence, '-' is empty ──
CHI="[CHI] n=120 max=0.7000 frac_gt_half=0.3100 mean=0.2100 rttvar_src=srtt_eighth"
ckeq "field n= is not read out of mean="   "120"    "$(field "$CHI" n)"
ckeq "field frac_gt_half"                  "0.3100" "$(field "$CHI" frac_gt_half)"
DIAG="[DIAG] t=9.9s rtt=41.2ms p0:infl=3 rtt=40/wrtt=42/rtp40ms"
ckeq "field rtt reads the head, not wrtt=" "41.2ms" "$(field "$DIAG" rtt)"
ckeq "field stops at '|'"                  "3"      "$(field "[X] gen=3|tail" gen)"
ckeq "field '-' is empty"                  ""       "$(field "[RFA] false_frac=- fires=0" false_frac)"
ckeq "field absent key is empty"           ""       "$(field "[RFA] fires=0" nope)"
ckeq "field on an empty line is empty"     ""       "$(field "" n)"

# ── lastline / countlines ──
printf '[RFA] fires=1\n\033[1m[RFA]\033[0m fires=2\r\n[DIAG] t=1\n' > "$TD/log"
ckeq "lastline strips colour and CR" "[RFA] fires=2" "$(lastline "$TD/log" '\[RFA\]')"
ckeq "lastline on a missing file is empty" "" "$(lastline "$TD/none" '\[RFA\]')"
ckeq "countlines" "2" "$(countlines "$TD/log" '\[RFA\]')"
ckeq "countlines on a missing file is 0, not empty" "0" "$(countlines "$TD/none" x)"

# ── count_final: the exit-flush count ──
{
  printf '[LAT] site=receiver n=5\n'
  printf '[LAT] site=receiver n=9 final=1\n'
  printf '\033[2m2026-09-08T18:54:50Z\033[0m INFO x [SUCC] det=7 final=1\r\n'
  printf '[ETA] site=receiver n=3 final=1\033[2m2026-09-08T18:54:50.1Z\033[0m  INFO cleaning up TUN interface\n'
  printf '[SUCC] det=7 final=10\n'
  printf '[SUCC] det=7 xfinal=1\n'
  printf '[ETA] site=sender n=3 final=1\n'
} > "$TD/srv.log"
ckeq "count_final: plain, prefixed, glued-timestamp flushes; not final=10/xfinal=1" "3" \
  "$(count_final "$TD/srv.log" '\[(LAT\] site=receiver|SUCC\]|ETA\] site=receiver)')"
ckeq "count_final: the sender's [ETA] is its own kind" "1" "$(count_final "$TD/srv.log" '\[ETA\] site=sender')"
ckeq "count_final on a missing file is 0" "0" "$(count_final "$TD/none" '\[LAT\]')"

# ── crlf_guard ──
printf 'echo hi\r\n' > "$TD/crlf.sh"; printf 'echo hi\n' > "$TD/lf.sh"
( crlf_guard "$TD/lf.sh" >/dev/null ) && ok "crlf_guard passes an LF file" || bad "crlf_guard refused an LF file"
( crlf_guard "$TD/lf.sh" "$TD/crlf.sh" >/dev/null ); ckeq "crlf_guard refuses a CR file (exit 3)" "3" "$?"

# ── sentinels ──
mkdir -p "$TD/out"
OUTP=$(prove_sentinels "$TD/out" DONE-ALL FAILED-ALL)
case "$OUTP" in *SENTINEL-PROOF-COMPLETE*) ok "prove_sentinels writes the proof line" ;; *) bad "no proof line" ;; esac
ckeq "probes leave nothing behind" "0" "$(find "$TD/out" -type f | wc -l | tr -d ' ')"
: > "$TD/out/r-s42.log"
seed_done 42 "$TD/out/r-s42.log" "R-BATTERY-DONE seed=42" R-ALL >/dev/null
ckeq "seed_done: an EMPTY ledger fails" "FAILED-S42" "$(ls "$TD/out" | grep -E '^(DONE|FAILED)-S42$')"
rm -f "$TD/out/FAILED-S42"
printf 'x\nR-BATTERY-DONE seed=42 now\n' > "$TD/out/r-s42.log"
seed_done 42 "$TD/out/r-s42.log" "R-BATTERY-DONE seed=42" R-ALL >/dev/null
ckeq "seed_done: the terminal line EARNS DONE" "DONE-S42" "$(ls "$TD/out" | grep -E '^(DONE|FAILED)-S42$')"

# ── locks: take, refuse a held lock, release on exit ──
L="$TD/lock"
( LB_TAG=t1; take_lock "$L" >/dev/null; release_locks >/dev/null )
[ ! -e "$L" ] && ok "release_locks removes the lock" || bad "lock left behind"
echo "999 other" > "$L"
( take_lock "$L" >/dev/null ); ckeq "a held lock is refused (exit 4)" "4" "$?"
ckeq "a refused lock is not removed" "999 other" "$(cat "$L")"
rm -f "$L"
( install_lock_traps; take_lock "$L" >/dev/null; exit 0 ) >/dev/null
[ ! -e "$L" ] && ok "the EXIT trap releases the lock" || bad "EXIT trap left the lock"
bash -c "source ./lib_battery.sh; install_lock_traps; take_lock '$L' >/dev/null; kill -TERM \$\$; sleep 5; echo RESUMED" > "$TD/term.out" 2>&1
RC=$?
ckeq "TERM exits 143" "143" "$RC"
[ ! -e "$L" ] && ok "TERM releases the lock" || bad "TERM left the lock"
grep -q RESUMED "$TD/term.out" && bad "TERM resumed the script" || ok "TERM does not resume the script"

# ── preflight_binary ──
printf '#!/bin/sh\n# RWM_PLACE_HOL\nexit 0\n' > "$TD/bin"; chmod +x "$TD/bin"
( preflight_binary "$TD/bin" RWM_PLACE_HOL >/dev/null 2>&1 ) && ok "preflight passes a binary carrying the gate" || bad "preflight refused a good binary"
( preflight_binary "$TD/bin" RWM_NOPE >/dev/null 2>&1 ); ckeq "preflight refuses a missing gate (exit 5)" "5" "$?"
( preflight_binary "$TD/missing" >/dev/null 2>&1 ); ckeq "preflight refuses a missing binary (exit 4)" "4" "$?"

if [ "$FAIL" -eq 0 ]; then echo "test_lib_battery.sh: ALL CHECKS PASS"; else echo "test_lib_battery.sh: FAILURES ABOVE"; fi
exit "$FAIL"
