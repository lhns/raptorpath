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
#   LB_LOG   a ledger file that lock / preflight messages are ALSO appended to

# Echo to stdout and, when LB_LOG is set, append to the ledger too.
_lb_say() {
  echo "$*"
  if [ -n "${LB_LOG:-}" ]; then echo "$*" >> "$LB_LOG" 2>/dev/null; fi
  return 0
}

# ── CRLF GUARD ──────────────────────────────────────────────────────────
# A tree synced without the CRLF repair runs `$'\r'`-suffixed commands and
# produces a ledger of garbage that looks like a result. Refuse instead.
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

# ── SENTINELS: writability is PROVEN at launch, not discovered at exit ──
# Probe the PATH, not the directory, as the user who will write it: create
# AND remove, because a sentinel that cannot be removed at the next launch
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

# A seed's DONE sentinel is EARNED, never unconditional: the ledger must
# exist, be non-empty and carry the battery's own terminal line. The
# sentinels live beside the ledger; a failure is also noted in all-era.txt.
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

# ── OPERATOR LOCKS ──────────────────────────────────────────────────────
# Co-tenancy on the box under measurement manufactures the abort signature
# the batteries look for (MEASURED: 121 RUN-RETRY over 171 polled
# invocations against 0 over 80 unpolled), so a second battery must refuse.
LOCKS_TAKEN=""
take_lock() { # path
  local p="$1"
  if (set -o noclobber; : > "$p") 2>/dev/null; then
    echo "$$ ${LB_TAG:-$(basename "$0" .sh)} $(date -u +%FT%TZ)" > "$p" 2>/dev/null
    LOCKS_TAKEN="$LOCKS_TAKEN $p"
    _lb_say "LOCK-TAKEN $p"
    return 0
  fi
  _lb_say "ABORT-LOCK $p is held: $(cat "$p" 2>/dev/null)"
  _lb_say "NOTHING WAS RUN. Co-tenancy on the box under measurement manufactures the abort signature it looks for."
  release_locks
  exit 4
}
release_locks() {
  local p
  for p in $LOCKS_TAKEN; do
    rm -f "$p" 2>/dev/null && _lb_say "LOCK-RELEASED $p"
  done
  LOCKS_TAKEN=""
  return 0
}
# INT and TERM must EXIT after releasing: a trap that only released would let
# the battery run on, lockless, after the operator stopped it.
install_lock_traps() {
  trap 'release_locks' EXIT
  trap 'release_locks; exit 130' INT
  trap 'release_locks; exit 143' TERM
}

# ── PREFLIGHT: the binary exists, runs, and carries the gates the arms need ─
# `grep -a` on the binary itself, NOT `strings | grep -q`: under `pipefail`
# the early-exiting `grep -q` SIGPIPEs `strings` and a PRESENT gate reads as
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

# ── LOG READERS (the shell twins of l1common.py) ────────────────────────
# The last line of FILE matching the grep PATTERN, colour and CR stripped.
lastline() { # file pattern
  grep -a -- "$2" "$1" 2>/dev/null | tr -d '\r' | sed 's/\x1b\[[0-9;]*m//g' | tail -1
  return 0
}
# The token after `key=` where `key` STARTS a token (so `n` never matches
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
