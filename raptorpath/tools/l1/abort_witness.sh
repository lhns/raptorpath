#!/bin/bash
# The abort-cause witness: records why an L1 invocation produced no
# `[GATES]` line on either endpoint.
#
# ── What it is for ──────────────────────────────────────────────────────
# Batteries treat an invocation with no `[GATES]` line on either endpoint as
# an abort: no datum, no liveness verdict, in no denominator. That rule is
# sound only while aborts are independent of the arm. When the abort rate
# differs between arms, the exclusion is a selection on the treatment, and
# every number over the survivors is conditioned on an arm-dependent event.
# This witness records which pre-transfer step failed, so that correlation can
# be explained rather than assumed.
#
# ── What "no [GATES] on either endpoint" narrows the cause to ───────────
# `[GATES]` is emitted from one site — `net::run_impl`, immediately after the
# gates resolve, before the TUN is parsed and long before any packet — and
# `perf` reaches it on both roles (`perf::server` and `perf::client` each
# spawn `net::run_with_tun(...)`). The echo therefore happens within
# milliseconds of a successful process start. Both logs are truncated before
# the invocation, so empty logs mean the engine never started — not that the
# transfer failed. The abort is pre-transfer, and the candidate steps are few
# enough to instrument exhaustively:
#
#   busy_precheck  `perf_rwm_c.sh` `cleanup`s (pkill -x raptorpath) and then
#                  immediately `pgrep -x raptorpath`, exiting 3 on a hit. SIGTERM
#                  is not synchronous, so this is a race against the previous
#                  invocation's teardown — and teardown duration depends on the
#                  arm (an arm that changes recovery clocks changes what the
#                  sender is doing when it is asked to die). `aw_drain`
#                  measures it directly, on every invocation.
#   topo_up        `ip netns add` on a name whose predecessor has not finished
#                  being torn down, a veth `link add` on a leftover peer, a
#                  `sysctl`/`ip mptcp` failure — each aborts `topo*.sh` under
#                  `set -e` and leaves the namespaces absent, after which every
#                  `ip netns exec` below fails and both logs stay empty.
#   srv_bind       the server never reaches `:7000` within the 20 × 0.3 s poll.
#   cli_exec       `ip netns exec` / `timeout 700` returns non-zero with nothing
#                  in the client log.
#
# ── The contract ────────────────────────────────────────────────────────
# One per-invocation record at a fixed path (`$AW_FILE`, default
# `/tmp/rwm-abort.txt`), `key=value`, one line per key, values sanitized to a
# single line. Batteries copy it under a rep-unique name exactly as they copy
# `/tmp/rwm-q.txt` and `/tmp/rwm-ping.txt`. `abort_witness.py` reads it.
#
# Nothing here changes harness behaviour. Every function is a recorder: exit
# codes are preserved and re-returned, `set -e` semantics at the call sites are
# unchanged, and with `AW_FILE` unset the functions still run (they write to
# the default path). A witness that alters the thing it witnesses cannot clear
# a selection effect, it can only move it.
#
# Sourced by `perf_rwm_c.sh` and the `topo*.sh` scripts. Not sourced by
# `lib.sh`, so no driver changes behaviour by inheriting it, and
# `gate_forwarding_list_covers_the_engine_surface` keeps parsing a file this
# instrument never touches.

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

# Start a fresh record. Called once per invocation, by `perf_rwm_c.sh`, before
# anything that can fail — including its own `cleanup`.
aw_begin() { # tag
    rm -f "$AW_FILE" 2>/dev/null || true
    : > "$AW_FILE" 2>/dev/null || true
    aw_kv aw_version 1
    aw_kv aw_tag "${1:-}"
    aw_kv aw_started "$(date -u +%FT%TZ)"
    aw_kv aw_pid "$$"
    # The identity of the invocation, forwarded by the battery so the record
    # stands alone if it is ever read outside its battery.
    aw_kv aw_cell "${AW_CELL:-}"
    aw_kv aw_arm "${AW_ARM:-}"
    aw_kv aw_era "${AW_ERA:-}"
    aw_kv aw_seed "${SEED:-}"
    aw_kv aw_rep "${AW_REP:-}"
}

# First write wins. The earliest step that failed is the cause; everything
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
        # first cause turns out to be a symptom. The token is kept with the
        # detail — `abort_also` is a list of later causes, not of later prose.
        aw_kv abort_also "$*"
    fi
}

# Run a step, record its exit code and its stderr, and re-return the code so the
# caller's own control flow is byte-identical to what it was without the witness.
# `set -e` safety: this file is sourced by `topo.sh` and `topo_dual.sh`, both
# under `set -euo pipefail`, so a bare `cmd; rc=$?` inside a function would
# kill the caller on the very failure it was written to record. Every status is
# captured through an `&& rc=0 || rc=$?` list, which `set -e` exempts.
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

# ── The process-teardown race, measured in two halves ───────────────────
# A witness that waits for the survivors would decide the outcome it observes:
# `perf_rwm_c.sh` aborts when `pgrep -x raptorpath` hits, so any sleep before
# that `pgrep` reduces the abort rate. So:
#
#   aw_drain_probe   Instantaneous: no sleep, no branch, no effect. Runs on
#                    every invocation, right after the caller's `pkill` and
#                    before its `pgrep`, and records how many survivors the
#                    pre-check is about to see. If aborting arms show
#                    survivors at t = 0 and the others none, the arm
#                    correlation is explained; if both show none, the SIGTERM
#                    race is cleared and the cause is one of the other steps.
#   aw_drain_watch   Timed, and called only after the abort decision has been
#                    taken, so it cannot change it. Answers "would waiting
#                    have helped, and for how long".
: "${AW_DRAIN_SAMPLES:=50}"

# `set -e` safety, as in `aw_step`: the caller sources `lib.sh`, which runs
# `set -euo pipefail`, and `pgrep` exits 1 when nothing matches — the normal,
# healthy case here. A bare `n=$(pgrep … | wc -l | …)` would propagate that 1
# through `pipefail` and `set -e` would kill the caller, so the status is taken
# through an `|| …` list. The early return is an `if` rather than an `&&` list
# so its `set -e` exemption does not depend on where the test sits in the line.
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
    # make the next invocation fail.
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

# The netns / interface / qdisc / socket state, sampled at the failure and
# necessarily before `perf_rwm_c.sh`'s `trap cleanup EXIT` destroys both
# namespaces — which is why this lives here and not in any caller that only
# regains control after the trap has run.
aw_state() { # label
    local l="$1" ns
    aw_kv "state_${l}_netns" "$(ip netns list 2>&1 | tr '\n' ' ')"
    for ns in rp-cli rp-srv; do
        if [ "$(ip netns list 2>/dev/null | grep -c "^$ns")" -gt 0 ]; then
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
            # `|| true`, not `|| echo 0`: `grep -c` prints `0` and exits 1 on no
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

# ── The topology sanity ping ────────────────────────────────────────────
#
# The check's purpose is "namespace and route exist", not "zero loss". The data
# legs are shaped with Gilbert-Elliott loss, so a 2-packet no-retry ping aborts
# the invocation whenever both packets land in the bad state — upstream of the
# code under test. A loss draw is the cell doing what it was shaped to do and
# must not abort; a leg with no namespace, no address or no route still must.
# So the ping retries until at least one reply, bounded, with the bound sized
# from the loss process.
#
# ── The sizing arithmetic (the number's whole provenance) ───────────────
#
# netem `loss gemodel p q` with `h`/`k` defaulted is a two-state Gilbert-
# Elliott chain that drops a packet iff the chain is in the bad state, with
# `p` = P(good->bad) and `q` = P(bad->good) per packet. Hence
#
#     pi_bad     = p / (p + q)                     stationary bad probability
#     P(stay)    = 1 - q                           per-packet bad-state persistence
#     P(N lost)  = pi_bad * (1 - q)^(N-1)          N consecutive packets, all lost
#
# The ICMP echo request is the only half that crosses a lossy qdisc — the
# reverse (`srv*`) direction is shaped delay/rate only, never loss — so each
# attempt is exactly one draw of this chain and the attempts of a retry loop
# are consecutive draws, identical in law to consecutive `-c` packets.
#
# Every GE cell in `lib.sh::scenario_params`:
#
#     cell           p      q      pi_bad     1-q     P(2 lost)   P(26 lost)
#     c1/dc          0.05   50     0.000999   0.500   5.0e-4      3.0e-11
#     c2/wifi        1.3    50     0.025341   0.500   1.3e-2      7.6e-10
#     c2r100l5       2.63   50     0.049971   0.500   2.5e-2      1.5e-9
#     c3/lte         2      40     0.047619   0.600   2.9e-2      1.4e-7
#     c2r100l10      5.56   50     0.100072   0.500   5.0e-2      3.0e-9
#     c4/sat         3      30     0.090909   0.700   6.4e-2      1.2e-5
#   > c5/badwifi     5.3    30     0.150142   0.700   1.1e-1      2.0e-5   <- WORST
#
# `c5` is the worst cell on both terms that matter (highest `pi_bad` and,
# jointly with `c4`, the highest persistence `1-q = 0.70`), so it sizes the
# bound for every other cell at once.
#
#     N = 2 :  0.150142 * 0.70^1  = 1.05e-1
#     N = 26:  0.150142 * 0.70^25 = 2.01e-5   per leg
#              1 - (1 - 2.01e-5)^4 = 8.05e-5  per quad invocation (4 legs)
#
# So N = 26 keeps a spurious abort below 1e-4 for a whole four-legged
# invocation.
#
# ── Why a retry loop and not `ping -c 26` ───────────────────────────────
# Identical arithmetic, but the loop exits on the first reply, so the healthy
# path costs one packet and about 10 ms. It is also the only form whose retry
# semantics a stubbed `ping` can test, which `test_topo.sh` does.
#
# A genuinely dead leg still aborts, and fast: no namespace, no address or no
# route makes `ping` fail immediately (`Network is unreachable`) rather than
# time out, so all 26 attempts are spent in milliseconds. Only the
# route-exists-but-100%-loss leg pays the timeout.
: "${AW_PING_ATTEMPTS:=26}"
: "${AW_PING_WAIT:=1}"

# A recorded sanity ping that preserves the caller's exit status, so `set -e`
# behaviour at the call site is exactly what it was: the final attempt's status
# is re-returned, `set -e` still kills `up()` on a leg that never replied, and
# the rc + the output are still recorded under the same `ping_<label>*` keys the
# witness has always written.
aw_ping() { # ns peer label
    local ns="$1" peer="$2" label="$3" out rc i
    rc=1
    for ((i = 1; i <= AW_PING_ATTEMPTS; i++)); do
        out="$(ip netns exec "$ns" ping -c 1 -W "$AW_PING_WAIT" "$peer" 2>&1)" \
            && rc=0 || rc=$?
        [ "$rc" -eq 0 ] && break
    done
    # The loop counter runs one past the bound when every attempt failed; the
    # recorded column must read "26 of 26 spent", not "27".
    [ "$i" -gt "$AW_PING_ATTEMPTS" ] && i="$AW_PING_ATTEMPTS"
    aw_kv "ping_${label}_rc" "$rc"
    # How many draws this leg needed. `1` is the healthy case; anything above
    # it is a loss draw recorded as a retry.
    aw_kv "ping_${label}_attempts" "$i"
    aw_kv "ping_${label}_max_attempts" "$AW_PING_ATTEMPTS"
    aw_kv "ping_${label}" "$(printf '%s' "$out" | tail -2)"
    printf '%s\n' "$out" | tail -1
    return "$rc"
}
