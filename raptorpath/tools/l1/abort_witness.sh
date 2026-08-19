#!/bin/bash
# THE ABORT-CAUSE WITNESS — goal-gate "Candidates Battery — RESULTS", THE ABORT
# CLASS row: "NEEDS-MORE with a named instrument … the owed instrument is an
# abort-cause witness". This file is that instrument.
#
# ── WHAT IT IS FOR ──────────────────────────────────────────────────────
# Every L1 battery since the flip battery encodes the same abort rule: an
# invocation with NO `[GATES]` line on EITHER endpoint carries no datum, no
# liveness verdict, and sits in no denominator (`ccand_battery.sh:246-252`,
# `ccand_report.py:21`). That rule is sound only while the aborts are
# INDEPENDENT of the arm. At c8/seed 7 the Candidates Battery measured them
# at 20 % on the control against 75 % on the RACK arm — so the exclusion is a
# SELECTION ON THE TREATMENT there, and every number computed over the
# survivors at that cell is conditioned on an arm-dependent event.
#
# The class was carried in prose as "the seed-7 topo-ping abort class" for
# three batteries. THAT ATTRIBUTION IS NOT SUPPORTED BY THE HARNESS, and this
# file's first job is to stop asserting it:
#
#   * `topo.sh:82` and `topo_dual.sh:80-81` run their sanity pings as the LAST
#     statements of `up()`. A failing ping kills `topo*.sh` through `set -e` +
#     `pipefail` — AFTER both namespaces, both veth pairs, every address, every
#     qdisc and (dual) both MPTCP endpoints are already in place.
#   * `perf_rwm_c.sh:99` calls `bash ./topo_dual.sh up … >/dev/null 2>&1` and
#     NEVER READS ITS EXIT CODE. A non-zero `topo up` is therefore invisible to
#     the invocation and, when the only failing step was the trailing ping, it
#     is also INCONSEQUENTIAL: the topology it built is complete.
#
#   So a failed topo ping cannot, by itself, produce a `[GATES]`-less
#   invocation. Something else does, and this witness records WHICH.
#
# ── WHAT "NO [GATES] ON EITHER ENDPOINT" NARROWS THE CAUSE TO ───────────
# `[GATES]` is emitted from ONE site — `net::run_impl` (`net/mod.rs:1801-1809`),
# immediately after `RuntimeGates::resolve()`, before the TUN is parsed and long
# before any packet — and `perf` reaches it on BOTH roles (`perf::server` and
# `perf::client` each `tokio::spawn(net::run_with_tun(...))`, `perf.rs:110,228`).
# The echo therefore happens within milliseconds of a successful process start.
# Both logs are truncated by the caller before the invocation and re-created by
# `>` / `tee` here, so EMPTY logs mean the ENGINE NEVER STARTED — not that the
# transfer failed. The abort is PRE-TRANSFER, at or before process launch, and
# the candidate steps are few enough to instrument exhaustively:
#
#   busy_precheck  `perf_rwm_c.sh` `cleanup`s (pkill -x raptorpath) and then
#                  IMMEDIATELY `pgrep -x raptorpath`, exiting 3 on a hit. SIGTERM
#                  is not synchronous, so this is a RACE against the previous
#                  invocation's teardown — and teardown duration is an ARM
#                  PROPERTY (an arm that changes recovery clocks changes what the
#                  sender is doing when it is asked to die). This is the leading
#                  hypothesis for the arm correlation and `aw_drain` measures it
#                  DIRECTLY, on every invocation, armed or not.
#   topo_up        `ip netns add` on a name whose predecessor has not finished
#                  being torn down, a veth `link add` on a leftover peer, a
#                  `sysctl`/`ip mptcp` failure — each aborts `topo*.sh` under
#                  `set -e` and leaves the namespaces ABSENT, after which every
#                  `ip netns exec` below fails and both logs stay empty.
#   srv_bind       the server never reaches `:7000` within the 20 × 0.3 s poll.
#   cli_exec       `ip netns exec` / `timeout 700` returns non-zero with nothing
#                  in the client log.
#
# ── THE CONTRACT THIS FILE OFFERS ───────────────────────────────────────
# ONE per-invocation record at a FIXED path (`$AW_FILE`, default
# `/tmp/rwm-abort.txt`), `key=value`, one line per key, values sanitized to a
# single line. Batteries copy it under a rep-unique name exactly as they already
# copy `/tmp/rwm-q.txt` and `/tmp/rwm-ping.txt`, so the launch step collects it
# with the rest of the ledger and NO protocol changes. `abort_witness.py` reads
# it and `era_parse.py` surfaces it as the `abort_cause=` column.
#
# NOTHING HERE CHANGES HARNESS BEHAVIOUR. Every function is a recorder: exit
# codes are preserved and re-returned, `set -e` semantics at the call sites are
# unchanged, and with `AW_FILE` unset the functions still run (they simply write
# to the default path). This is deliberate — a witness that alters the thing it
# witnesses cannot clear a selection effect, it can only move it.
#
# Sourced by `perf_rwm_c.sh`, `topo.sh`, `topo_dual.sh` and `era_battery.sh`.
# NOT sourced by `lib.sh`, so no existing driver changes behaviour by inheriting
# it, and `gate_forwarding_list_covers_the_engine_surface` keeps parsing a file
# this instrument never touched.

: "${AW_FILE:=/tmp/rwm-abort.txt}"

# One-line-ify: the record is `key=value` per line and a captured stderr is
# routinely multi-line. Newlines become ' | ', control characters and ANSI SGR
# go away, and the value is truncated so one pathological step cannot bury the
# rest of the record.
: "${AW_MAXLEN:=600}"
aw_sanitize() {
    printf '%s' "$*" \
        | sed 's/\x1b\[[0-9;]*m//g' \
        | tr '\n\r\t' '   ' \
        | tr -cd '\11\12\15\40-\176' \
        | cut -c1-"$AW_MAXLEN"
}

aw_kv() { # key value...
    local k="$1"; shift
    printf '%s=%s\n' "$k" "$(aw_sanitize "$*")" >> "$AW_FILE" 2>/dev/null || true
}

# Start a fresh record. Called once per invocation, by `perf_rwm_c.sh`, BEFORE
# anything that can fail — including its own `cleanup`.
aw_begin() { # tag
    rm -f "$AW_FILE" 2>/dev/null || true
    : > "$AW_FILE" 2>/dev/null || true
    aw_kv aw_version 1
    aw_kv aw_tag "${1:-}"
    aw_kv aw_started "$(date -u +%FT%TZ)"
    aw_kv aw_pid "$$"
    # The identity of the invocation, forwarded by the battery so the record
    # stands alone if it is ever read outside its ledger.
    aw_kv aw_cell "${AW_CELL:-}"
    aw_kv aw_arm "${AW_ARM:-}"
    aw_kv aw_era "${AW_ERA:-}"
    aw_kv aw_seed "${SEED:-}"
    aw_kv aw_rep "${AW_REP:-}"
}

# FIRST WRITE WINS. The earliest step that failed is the cause; everything
# downstream of it is a consequence, and a witness that let the last failure
# overwrite the first would attribute every abort to `cli_exec`.
aw_cause() { # cause detail...
    if ! grep -q '^abort_cause=' "$AW_FILE" 2>/dev/null; then
        local c="$1"; shift
        aw_kv abort_cause "$c"
        aw_kv abort_detail "$*"
        aw_kv abort_at "$(date -u +%FT%TZ)"
    else
        # Kept, never scored: the consequence chain is informative when the
        # first cause turns out to be a symptom. The token is kept WITH the
        # detail — `abort_also` is a list of later causes, not of later prose.
        aw_kv abort_also "$*"
    fi
}

# Run a step, record its exit code and its stderr, and RE-RETURN the code so the
# caller's own control flow is byte-identical to what it was without the witness.
# NOTE ON `set -e` SAFETY, which is not decoration here: this file is sourced by
# `topo.sh` and `topo_dual.sh`, both of which run under `set -euo pipefail`. A
# bare `cmd; rc=$?` inside a function would therefore KILL THE CALLER on the
# very failure it was written to record. Every status is captured through an
# `&& rc=0 || rc=$?` list, which `set -e` exempts by definition.
aw_step() { # label cmd...
    local label="$1"; shift
    local err rc
    err="$(mktemp 2>/dev/null || echo /tmp/aw-err.$$)"
    "$@" 2>"$err" >/dev/null && rc=0 || rc=$?
    aw_kv "step_${label}_rc" "$rc"
    if [ "$rc" -ne 0 ]; then
        aw_kv "step_${label}_stderr" "$(cat "$err" 2>/dev/null)"
        aw_kv "step_${label}_cmd" "$*"
        aw_cause "$label" "rc=$rc $(head -c 200 "$err" 2>/dev/null)"
    fi
    rm -f "$err" 2>/dev/null || true
    return "$rc"
}

# ── THE PROCESS-TEARDOWN RACE, MEASURED IN TWO HALVES ───────────────────
# The halves exist because a witness that WAITS for the survivors would decide
# the very outcome it is supposed to observe: `perf_rwm_c.sh` aborts the
# invocation when `pgrep -x raptorpath` hits, so any sleep placed before that
# `pgrep` REDUCES the abort rate and destroys the class under study. So:
#
#   aw_drain_probe   INSTANTANEOUS. No sleep, no branch, no effect. Runs on
#                    EVERY invocation, right after the caller's `pkill` and
#                    before its `pgrep`, and records how many survivors the
#                    pre-check is ABOUT to see. This is the column with a
#                    control: if the aborting arms show survivors at t = 0 and
#                    the others show none, the arm correlation is EXPLAINED; if
#                    both show none, the SIGTERM race is CLEARED and the cause
#                    is one of the other three steps.
#   aw_drain_watch   TIMED, and called ONLY after the abort decision has already
#                    been taken — so it cannot change it. Answers "would waiting
#                    have helped, and for how long", which is what a later
#                    remedy commit needs and what this one deliberately does not
#                    act on.
: "${AW_DRAIN_SAMPLES:=50}"

# `set -e` SAFETY, AND IT IS THE SAME TRAP `aw_step` DOCUMENTS — this function
# fell into it anyway, and the era battery's smoke is what found it.
# `perf_rwm_c.sh` sources `lib.sh`, which runs `set -euo pipefail`, so BY THE
# TIME THIS FUNCTION IS CALLED THE CALLER IS UNDER `set -e` WITH `pipefail`.
# `pgrep` EXITS 1 WHEN NOTHING MATCHES — which is the NORMAL, HEALTHY case here,
# because the caller has just `pkill`ed and is about to check that the box is
# idle. A bare `n=$(pgrep … | wc -l | …)` is a simple command whose status is
# the pipeline's, `pipefail` propagates `pgrep`'s 1 past `wc`'s 0, and `set -e`
# then kills the caller. The witness measured 6/6 invocations dying at this line
# with `abort_cause=none` and a record that stopped at `bin=`: A RECORDER THAT
# ABORTS THE INVOCATION IT IS RECORDING, which is the exact failure this file's
# own header promises cannot happen ("NOTHING HERE CHANGES HARNESS BEHAVIOUR").
# The status is therefore taken through an `|| …` list, which `set -e` exempts
# by definition.
#
# The early return became an `if` in the same edit, and that half is HYGIENE,
# NOT A SECOND BUG: a false `[ … ]` at the head of an `&&` list is exempt from
# `set -e` because it is not the list's last command, and the gate confirms it —
# `test_abort_witness.sh` case 2 passes against the pre-fix file. It is written
# as an `if` because the exemption is a property of where the test SITS in the
# line rather than of what it does, and this function has now cost one battery
# on exactly that distinction.
aw_drain_probe() {
    local n
    n=$(pgrep -x raptorpath 2>/dev/null | wc -l | tr -d ' ') || n=0
    aw_kv drain_pids_t0 "${n:-0}"
    if [ "${n:-0}" -eq 0 ]; then return 0; fi
    aw_kv drain_cmdlines_t0 "$(for p in $(pgrep -x raptorpath 2>/dev/null); do
            tr '\0' ' ' < "/proc/$p/cmdline" 2>/dev/null; echo -n ' ;; '
        done)"
    # The states matter: a `Z`ombie is an un-reaped child and holds no port or
    # namespace; an `R`/`D`/`S` survivor still holds both, and only that one can
    # make the NEXT invocation fail.
    aw_kv drain_states_t0 "$(for p in $(pgrep -x raptorpath 2>/dev/null); do
            awk '{print $3}' "/proc/$p/stat" 2>/dev/null; echo -n ' '
        done)"
    return 0
}

aw_drain_watch() {
    local t0 i gone=-1
    t0=$(date +%s%3N 2>/dev/null || echo 0)
    for i in $(seq 1 "$AW_DRAIN_SAMPLES"); do
        pgrep -x raptorpath >/dev/null 2>&1 || { gone=$i; break; }
        sleep 0.01
    done
    if [ "$gone" -ge 0 ]; then
        aw_kv drain_ms "$(( $(date +%s%3N 2>/dev/null || echo 0) - t0 ))"
        aw_kv drain_left 0
    else
        aw_kv drain_ms ">$(( AW_DRAIN_SAMPLES * 10 ))"
        aw_kv drain_left "$(pgrep -x raptorpath 2>/dev/null | wc -l | tr -d ' ')"
    fi
}

# The netns / interface / qdisc / socket state, sampled AT the failure and
# necessarily BEFORE `perf_rwm_c.sh`'s `trap cleanup EXIT` destroys both
# namespaces — which is why this lives here and not in any caller that only
# regains control after the trap has run.
aw_state() { # label
    local l="$1" ns
    aw_kv "state_${l}_netns" "$(ip netns list 2>&1 | tr '\n' ' ')"
    for ns in rp-cli rp-srv; do
        if ip netns list 2>/dev/null | grep -q "^$ns"; then
            aw_kv "state_${l}_${ns}_links" "$(ip -n "$ns" -br addr 2>&1 | tr '\n' ' ')"
            aw_kv "state_${l}_${ns}_qdisc" "$(ip netns exec "$ns" tc qdisc show 2>&1 | grep -v noqueue | tr '\n' ' ')"
            aw_kv "state_${l}_${ns}_sock" "$(ip netns exec "$ns" ss -uln 2>&1 | tr '\n' ' ')"
        else
            aw_kv "state_${l}_${ns}" ABSENT
        fi
    done
    aw_kv "state_${l}_procs" "$(pgrep -x raptorpath 2>/dev/null | tr '\n' ' ')"
    aw_kv "state_${l}_load" "$(cut -d' ' -f1-3 /proc/loadavg 2>/dev/null)"
}

# Sizes and first lines of the two engine logs — the direct evidence for
# "the process never started" against "it started and said nothing".
aw_logs() { # label
    local l="$1" f n
    for f in /tmp/rwm-c.log:cli /tmp/rwm-s.log:srv; do
        n="${f#*:}"; f="${f%%:*}"
        if [ -f "$f" ]; then
            aw_kv "log_${l}_${n}_bytes" "$(wc -c < "$f" 2>/dev/null | tr -d ' ')"
            # `|| true`, NOT `|| echo 0`: `grep -c` prints `0` AND exits 1 on no
            # match, so the `echo` idiom appends a second zero.
            aw_kv "log_${l}_${n}_gates" "$(grep -c '\[GATES\]' "$f" 2>/dev/null || true)"
            aw_kv "log_${l}_${n}_head" "$(head -3 "$f" 2>/dev/null)"
        else
            aw_kv "log_${l}_${n}" MISSING
        fi
    done
}

# The `set -E` ERR-trap body for `topo.sh` / `topo_dual.sh`: names the exact
# failing command and line inside `up()`, which is the one thing a swallowed
# `>/dev/null 2>&1` exit code can never tell the caller.
aw_err_trap() { # rc line command
    aw_kv topo_fail_rc "$1"
    aw_kv topo_fail_line "$2"
    aw_kv topo_fail_cmd "$3"
    aw_cause "topo_step" "line=$2 rc=$1 cmd=$3"
}

# A recorded sanity ping that PRESERVES the caller's exit status, so `set -e`
# behaviour at the call site is exactly what it was.
aw_ping() { # ns peer label
    local ns="$1" peer="$2" label="$3" out rc
    out="$(ip netns exec "$ns" ping -c 2 -i 0.2 -W 2 "$peer" 2>&1)" && rc=0 || rc=$?
    aw_kv "ping_${label}_rc" "$rc"
    aw_kv "ping_${label}" "$(printf '%s' "$out" | tail -2)"
    printf '%s\n' "$out" | tail -1
    return "$rc"
}
