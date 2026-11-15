#!/bin/bash
# Shared plumbing for the L1 battery drivers (r_battery.sh, recvlaw_battery.sh,
# place_battery.sh, place_run_all.sh). SOURCE it; it defines functions only.
#
# It deliberately sets NO shell options: a battery runs with per-arm abort
# tolerance (`set +e`), and a sourced library that turned `set -e` back on
# would make the first failing arm kill the whole battery.
#
# Knobs the caller may set before calling:
#   LB_TAG   owner tag written into a lock file   (default: the script name)
#   LB_LOG   a log file that lock / preflight messages are also appended to

# Echo to stdout and, when LB_LOG is set, append to the log too.
_lb_say() {
  echo "$*"
  if [ -n "${LB_LOG:-}" ]; then echo "$*" >> "$LB_LOG" 2>/dev/null; fi
  return 0
}

# ── CRLF GUARD ──────────────────────────────────────────────────────────
# A tree synced without the CRLF repair runs `$'\r'`-suffixed commands and
# produces a log of garbage that looks like a result. Refuse instead.
crlf_guard() { # file...
  local f
  for f in "$@"; do
    # -U: do not strip CR at EOL (a no-op on Linux; MSYS grep strips it).
    if [ -f "$f" ] && LC_ALL=C grep -qU $'\r' "$f" 2>/dev/null; then
      echo "ABORT-CRLF $f carries CR bytes -- the tree was synced without the CRLF repair."
      echo "NOTHING WAS RUN. Repair the tree (dos2unix) and relaunch."
      exit 3
    fi
  done
  return 0
}

# ── Sentinels: writability is proven at launch, not discovered at exit ──
# Probe the path, not the directory, as the user who will write it: create
# and remove, because a sentinel that cannot be removed at the next launch
# reports the previous run's verdict.
probe_sentinel() { # path
  local p="$1" d
  d="$(dirname "$p")"
  if : > "$p.probe" 2>/dev/null && rm -f "$p.probe" 2>/dev/null; then
    echo "SENTINEL-WRITABLE $p (probed as $(id -un), create+unlink)"
    return 0
  fi
  echo "ABORT-SENTINEL-UNWRITABLE $p"
  echo "ABORT-SENTINEL-UNWRITABLE dir=$d owner=$(stat -c '%U:%G %a' "$d" 2>/dev/null) user=$(id -un)"
  echo "NOTHING WAS RUN. Fix the ownership of $d and relaunch: a pass whose sentinel cannot be written is a pass whose completion cannot be observed."
  exit 3
}

# Probe every named sentinel under DIR, then write the proof line. Any extra
# context for the proof line goes in LB_PROOF_EXTRA.
prove_sentinels() { # dir name...
  local d="$1" n
  shift
  for n in "$@"; do probe_sentinel "$d/$n"; done
  echo "SENTINEL-PROOF-COMPLETE $(date -u +%FT%TZ) user=$(id -un) dir=$d ${LB_PROOF_EXTRA:-}"
}

# A seed's DONE sentinel is earned, never unconditional: the log must exist,
# be non-empty and carry the battery's own terminal line. The sentinels live
# beside the log; a failure is also noted in all-era.txt.
seed_done() { # seed ledger done_mark [message-prefix]
  local s="$1" f="$2" mark="$3" pfx="${4:-BATTERY}" d
  d="$(dirname "$f")"
  if [ -s "$f" ] && grep -aqF -- "$mark" "$f" 2>/dev/null; then
    touch "$d/DONE-S$s"
    return 0
  fi
  echo "$pfx seed $s DID NOT COMPLETE -- no '$mark' in $f" | tee -a "$d/all-era.txt"
  touch "$d/FAILED-S$s"
  return 1
}

# ── Operator locks ──────────────────────────────────────────────────────
# Co-tenancy on the box under measurement manufactures the abort signature
# the batteries look for (docs/measurement-discipline.md rules 12-13), so a
# second battery must refuse.
#
# One convention, two witnesses (docs/measurement-discipline.md, VM protocol):
#   1. the lock is HELD while the file EXISTS -- it is created with noclobber,
#      so a second creator fails, and removed on release;
#   2. the holder also keeps flock(2) on the file through an fd opened `<>`
#      (read-write, NO truncation), so a script that locks with
#      `exec 8>PATH; flock -n 8` finds it held and runs nothing.
# `>` on an existing lock file truncates the owner's token; never do it.
LOCKS_TAKEN=""
declare -gA LB_LOCK_FD=()
_lb_lock_refuse() { # path reason
  _lb_say "ABORT-LOCK $1 $2"
  _lb_say "NOTHING WAS RUN. Co-tenancy on the box under measurement manufactures the abort signature it looks for."
  release_locks
  exit 4
}
take_lock() { # path
  local p="$1" fd
  command -v flock >/dev/null 2>&1 || _lb_lock_refuse "$p" "cannot be taken: flock(1) is not installed"
  if ! (set -o noclobber; : > "$p") 2>/dev/null; then
    _lb_lock_refuse "$p" "is held: $(cat "$p" 2>/dev/null)"
  fi
  if ! exec {fd}<>"$p" 2>/dev/null; then
    rm -f "$p" 2>/dev/null
    _lb_lock_refuse "$p" "was created but cannot be opened <> for flock"
  fi
  if ! flock -n "$fd"; then
    # Someone opened the fresh file and took flock(2) between our create and
    # our open: they hold it. Leave the file to them.
    exec {fd}>&-
    _lb_lock_refuse "$p" "is held by a foreign flock(2) holder"
  fi
  echo "$$ ${LB_TAG:-$(basename "$0" .sh)} $(date -u +%FT%TZ)" >&"$fd"
  LB_LOCK_FD["$p"]="$fd"
  LOCKS_TAKEN="$LOCKS_TAKEN $p"
  _lb_say "LOCK-TAKEN $p (noclobber + flock fd $fd)"
  return 0
}
# Remove first, then close: a process that inherited the fd keeps flock(2) on
# the unlinked inode only, which no later locker can open.
release_locks() {
  local p fd
  for p in $LOCKS_TAKEN; do
    rm -f "$p" 2>/dev/null && _lb_say "LOCK-RELEASED $p"
    fd="${LB_LOCK_FD[$p]:-}"
    if [ -n "$fd" ]; then exec {fd}>&-; fi
    unset 'LB_LOCK_FD[$p]'
  done
  LOCKS_TAKEN=""
  return 0
}
# INT and TERM must exit after releasing: a trap that only released would let
# the battery run on, lockless, after the operator stopped it.
install_lock_traps() {
  trap 'release_locks' EXIT
  trap 'release_locks; exit 130' INT
  trap 'release_locks; exit 143' TERM
}

# ── Preflight: the binary exists, runs, and carries the gates the arms need ─
# `grep -a` on the binary itself, not `strings | grep -q`: under `pipefail`
# the early-exiting `grep -q` SIGPIPEs `strings` and a present gate reads as
# absent.
preflight_binary() { # bin gate...
  local bin="$1" g
  shift
  if [ ! -x "$bin" ]; then
    _lb_say "REFUSED: no engine binary at $bin" >&2; exit 4
  fi
  if ! "$bin" --help >/dev/null 2>&1; then
    _lb_say "REFUSED: engine binary will not run: $bin" >&2; exit 4
  fi
  for g in "$@"; do
    if ! grep -aq -- "$g" "$bin" 2>/dev/null; then
      _lb_say "REFUSED: $g is not present in the binary -- this is the OLD ENGINE" >&2
      exit 5
    fi
  done
  return 0
}

# ── Log readers (the shell twins of l1common.py) ────────────────────────
# The last line of FILE matching the grep PATTERN, colour and CR stripped.
lastline() { # file pattern
  grep -a -- "$2" "$1" 2>/dev/null | tr -d '\r' | sed 's/\x1b\[[0-9;]*m//g' | tail -1
  return 0
}
# The token after `key=` where `key` starts a token (so `n` never matches
# inside `mean=` or `gen=`), first occurrence, up to whitespace or `|`.
# `-` (the engine's n = 0 rendering) and an absent key both print nothing.
field() { # text key
  local re="(^|[[:space:]])$2=([^[:space:]|]*)" v
  if [[ $1 =~ $re ]]; then
    v="${BASH_REMATCH[2]}"
    [ "$v" = "-" ] || printf '%s' "$v"
  fi
  return 0
}
# How many lines of FILE match PATTERN; 0 (never empty) for a missing file.
countlines() { # file pattern
  local n
  n=$(grep -ac -- "$2" "$1" 2>/dev/null)
  echo "${n:-0}"
}
# How many exit-flush lines (`final=1`) of the kind TAG_ERE FILE carries; 0
# for a missing file. Colour and CR are stripped first, the tag need not
# start the line (a tracing prefix may precede it), and `final=1` counts when
# a tracing record's timestamp is glued onto it (`final=12026-09-08T...`) --
# `final=10` / `xfinal=1` do not.
count_final() { # file tag_ere
  local n
  n=$(sed 's/\x1b\[[0-9;]*m//g; s/\r//g' "$1" 2>/dev/null | grep -aE -- "$2" \
      | grep -acE '(^|[[:space:]])final=1([[:space:]]|$|[0-9]{4}-[0-9]{2}-[0-9]{2}T)')
  echo "${n:-0}"
}
