#!/bin/bash
# RWM Phase C measurement: `raptorpath perf` (rp-native objects) over the
# RELIABLE sliding-window pipeline, single OR dual path, with an optional
# OUT-OF-ORDER object delivery toggle (paper §16.2 H->inf corner).
#
#   sudo bash perf_rwm_c.sh <scenA> <scenB> <hint> <bytes> <runs> <dual|single|quad> [T]
#
#   env RWM_OOO=1     -> add --window-out-of-order (H->inf, decode-on-total)
#   env RWM_EXTRA=".." -> extra CLI args appended to server+client (raise-r arm)
#   env RWM_PLACE_T=.. -> placement-temperature override (via 7th arg too)
#
#   C7 = c2 c2   C8 = c2 c3
#
# QUAD MODE (feat/c9-quad-cell). `quad` runs FOUR veth legs via topo_quad.sh,
# and the two scenario arguments are the two LEG CLASSES — each is used for
# TWO legs, in order: `<scenA> <scenA> <scenB> <scenB>`. So
#
#   C9  = c2 c2 ... quad   ->  c2 c2 c2 c2   the SYMMETRIC quad
#   C9H = c2 c3 ... quad   ->  c2 c2 c3 c3   the HETEROGENEOUS quad (C9-3)
#
# STATED RATHER THAN IMPLIED: this parameterization expresses 2 + 2 geometries
# ONLY. A quad of four distinct classes, or a 3 + 1 split, is NOT reachable
# through this script and must not be faked by a caller — it would need a
# fourth positional argument, and the two registered c9 geometries do not.
# `topo_quad.sh` itself takes four independent scenarios, so the restriction
# is this driver's signature, not the topology's.
set -uo pipefail
cd "$(dirname "$0")"
source ./lib.sh
# THE ABORT-CAUSE WITNESS (goal-gate "Candidates Battery — RESULTS", THE ABORT
# CLASS row's named instrument). Recorders only: every capture below preserves
# the exit code it observed and changes no control flow. See abort_witness.sh
# for why "no [GATES] on either endpoint" narrows the cause to the four
# pre-transfer steps instrumented here, and why the "topo-ping" attribution the
# class has carried for three batteries is not supported by this harness.
source ./abort_witness.sh
# THE BINARY, OVERRIDABLE — goal-gate "Era Battery — PRE-REGISTRATION", THE
# TWO-BINARY PROTOCOL. Every battery before this one scored arms of ONE binary
# and could hard-code the path; the era battery runs a SECOND, older era's
# binary as its baseline arm. The DEFAULT IS UNCHANGED, so every existing driver
# is byte-identical in behaviour, and `$BIN` is echoed into the witness so the
# ledger can prove WHICH binary an invocation ran.
BIN="${RWM_BIN:-/home/vibe/raptorpath/target/release/raptorpath}"
SCENA="${1:?scenA}"; SCENB="${2:?scenB}"; HINT="${3:-bulk}"
BYTES="${4:-1800000}"; RUNS="${5:-10}"; MODE="${6:-dual}"; PLACE_T="${7:-}"

# ── GATE FORWARDING (goal-gate "Gate-Forwarding Audit", 2026-08-09) ──────
# ONE shared list, in lib.sh, sourced above. This block used to be 78 lines
# of hand-rolled `[[ -n "${RWM_X:-}" ]] && TENV="$TENV RWM_X=$RWM_X"`, and
# the audit found 12 engine gates that had never been added to it —
# RWM_ACK_MERGE (found by the 2026-08-08 flip battery) plus RWM_RECOV_SP,
# RWM_RECOV_MP_LIVE, RWM_PLACE_SLACK, RWM_PATIENCE_DERIVED,
# RWM_SIDLE_DERIVED, RWM_SCHED_SNAPSHOT, RWM_STORE_BOOT and the four
# RWM_COPA_* knobs. Those arms all measured correctly ANYWAY, by plain
# process-environment inheritance (MEASURED, PROBE 0 + runs A/B/D of the
# audit sweep: the six non-allowlisted echo-bearing gates fire on BOTH logs
# when set and on NEITHER when unset) — but a harness that CAN drop a knob
# is a harness that will, so the forwarding is now total and explicit.
TENV="$(rwm_forward_env)"
[[ -n "$PLACE_T" ]] && TENV="$TENV RWM_PLACE_T=$PLACE_T"

# feat/gen-on-rebaseline NAME-COLLISION NOTE: the binary reads RWM_GEN as the
# generation SIZE G (gates.rs, default 384, `.max(1)`). This harness ALSO uses
# RWM_GEN as the on/off GATE for --window-generation-coding: RWM_GEN=0 -> plain
# window control, unset/1 -> generation ON at the binary's default G.
#
# The sentinels 0 and 1 must therefore NOT reach the binary (=1 would set a
# catastrophic 1-symbol generation, =0 a 0-symbol one clamped to 1). The old
# code tried to achieve that by OMITTING them from the allowlist — which the
# audit showed does nothing, because the binary inherits this script's whole
# environment regardless. The only way to withhold a var is to remove it from
# OUR OWN environment, which is what `unset` below does; a real generation
# size (>=2) is forwarded normally by rwm_forward_env.
GEN_GATE="${RWM_GEN:-1}"
if [[ "${RWM_GEN:-}" == "0" || "${RWM_GEN:-}" == "1" ]]; then
    unset RWM_GEN
    TENV="$(rwm_forward_env)"
    [[ -n "$PLACE_T" ]] && TENV="$TENV RWM_PLACE_T=$PLACE_T"
fi

OOO_FLAG=""
[[ "${RWM_OOO:-0}" == "1" ]] && OOO_FLAG="--window-out-of-order"
EXTRA="${RWM_EXTRA:-}"

# feat/gen-on-rebaseline: GENERATION is FIRST-CLASS in the aggregation harness.
# The coded/generation pipeline (and therefore DAPS, the per-path rate-sample
# estimator, the read-ahead depth bound, source-backpressure — EVERYTHING the
# §16.11-16.14 arc measured) is enabled ONLY by the --window-generation-coding CLI
# flag: net/mod.rs:701 gates window_generation on
#   window_reliable && (window_generation_coding || window_systematic_repair)
# and RWM_DAPS/RWM_GEN_R/RWM_RATE_SAMPLE only *configure* generation, they do NOT
# *enable* it (`daps = RWM_DAPS && generation`).  The §16.14 diagnosis proved this
# harness NEVER passed that flag, so the entire recent arc ran with the coded path
# DEAD (cod=0).  Generation now DEFAULTS ON here; set RWM_GEN=0 for the plain-window
# control.  --window-reliable is kept (generation requires it, main.rs:302).
GEN_FLAG="--window-generation-coding"
# GEN_GATE, not RWM_GEN: the sentinel branch above `unset`s RWM_GEN so it cannot
# reach the binary as a 1-symbol generation size, so the GATE meaning must be
# read from the saved copy.
[[ "$GEN_GATE" == "0" ]] && GEN_FLAG=""
# Force the cumulative coded-emission counter on so the HARD SANITY GUARD (below)
# can assert cod>0 on the SENDER.  RWM_PFRAC makes run_window_sender print
# "[PFRAC] ... total_coded=N ..." every 500 ms (generation-gated, cheap).
if [[ -n "$GEN_FLAG" && -z "${RWM_PFRAC:-}" ]]; then
    TENV="$TENV RWM_PFRAC=1"
fi

# The DEVICE LISTS this invocation's topology owns. Set with the mode, read by
# the qdisc captures at the bottom — ONE definition, so a capture cannot go on
# reading two legs after the topology grew to four. That is the `pid < 2`
# defect the SF bench taught (`MAX_PATHS` widened 2 -> 4 while three per-path
# gauge guards kept their hard-coded `< 2`, so two legs read 0 no matter what
# placement did and the assertion built on them could not fail); the fix there
# was to derive the bound from the path count, and this is the same fix in the
# harness.
case "$MODE" in
    quad)   CLI_LEGS=(cli0 cli1 cli2 cli3); SRV_LEGS=(srv0 srv1 srv2 srv3) ;;
    dual)   CLI_LEGS=(cli0 cli1);           SRV_LEGS=(srv0 srv1) ;;
    single) CLI_LEGS=(cli0);                SRV_LEGS=(srv0) ;;
    *) echo "unknown mode '$MODE' (want single|dual|quad)" >&2; exit 2 ;;
esac
TOPO=./topo_dual.sh
[[ "$MODE" == "quad" ]] && TOPO=./topo_quad.sh

cleanup() {
    pkill -x raptorpath 2>/dev/null || true
    # BOTH topologies are torn down regardless of this invocation's mode: they
    # share the rp-cli/rp-srv namespaces, so a quad left behind by a crashed
    # run would otherwise be inherited by the next dual run as a four-legged
    # cell wearing a two-legged cell's name. `down` is idempotent and deletes
    # the namespaces, so the second call is a no-op.
    bash ./topo_dual.sh down >/dev/null 2>&1 || true
    bash ./topo_quad.sh down >/dev/null 2>&1 || true
}
trap cleanup EXIT
# ARM THE WITNESS BEFORE THE FIRST THING THAT CAN FAIL — which is `cleanup`
# itself, whose `pkill` is the opening move of the SIGTERM race the `BUSY`
# pre-check below loses.
aw_begin "perf_rwm_c $SCENA/$SCENB/$MODE $BYTES"
aw_kv bin "$BIN"
cleanup
# THE TEARDOWN RACE, MEASURED ON EVERY INVOCATION (not only on aborts, or the
# column would have no control to be read against). `cleanup` has just sent
# SIGTERM; the `pgrep` below aborts the invocation if anything is still alive.
# SIGTERM is not synchronous and shutdown duration is an ARM PROPERTY, so this
# is the leading candidate for the c8/seed-7 arm correlation (20 % control vs
# 75 % RACK). THE PROBE IS INSTANTANEOUS BY CONSTRUCTION — a witness that slept
# here would lower the abort rate and destroy the class it exists to explain.
aw_drain_probe
# The tc capture below writes a FIXED path, so a run that aborts before
# reaching it would leave the PREVIOUS invocation's counters there for the
# caller to copy under this cell's name. Silently attributing one cell's
# wire truth to another is worse than having no capture, so clear it first:
# an absent file is then an unambiguous "this invocation produced none".
rm -f /tmp/rwm-q.txt

if pgrep -x raptorpath >/dev/null 2>&1; then
    echo "BUSY: raptorpath already running -- aborting" >&2
    # THE FIRST OF THE FOUR PRE-TRANSFER ABORT CAUSES, and the only one that
    # produces NO log file at all on either endpoint. The decision has already
    # been taken above; everything here is recorded after it.
    aw_cause busy_precheck "pgrep hit after cleanup's SIGTERM"
    aw_state busy
    aw_drain_watch
    exit 3
fi

# `topo*.sh up`'s exit code was DISCARDED here (`>/dev/null 2>&1`, unchecked)
# through every battery this tree has run — so a namespace that failed to come
# up was indistinguishable from one that did, and the invocation carried on to
# `ip netns exec` against nothing. The witness records the code and the stderr;
# the CONTROL FLOW IS UNCHANGED (`aw_step` re-returns the code and nothing here
# reads it), because changing it now would move the abort class instead of
# explaining it.
#
# SEED is a per-leg SPEC now, not one value (lib.sh's HARNESS ERA note): a
# bare `42` derives 42/1042/2042/3042 and gives every leg its own netem
# realization. It is passed through verbatim so a caller can still pin the
# legs equal (`SEED=42,42`) for the rho_loss = +1 arm.
if [[ "$MODE" == "quad" ]]; then
    aw_step topo_up bash "$TOPO" up "$SCENA" "$SCENA" "$SCENB" "$SCENB" \
        --seed "${SEED:-42}"
else
    aw_step topo_up bash "$TOPO" up "$SCENA" "$SCENB" --seed "${SEED:-42}"
fi
aw_state post_topo

# Bind/peer lists are BUILT FROM THE LEG LIST, not written out per mode: the
# addressing stride is 10.(77+i).0.x, so the address set and the device set
# cannot drift apart when a leg is added.
SRV_BIND=""; PEERS=""; CLI_BIND=""
for ((li = 0; li < ${#CLI_LEGS[@]}; li++)); do
    SRV_BIND="${SRV_BIND}${SRV_BIND:+,}10.$((77 + li)).0.2:7000"
    CLI_BIND="${CLI_BIND}${CLI_BIND:+,}10.$((77 + li)).0.1:0"
done
PEERS="$SRV_BIND"

# LOG SOURCES (feat/gen-on-rebaseline; the §16.14 wrong-log trap): the --server is
# the perf RECEIVER of the bulk transfer (its reverse sender loop places ~no source,
# ~no coded — reading sender-side counters here is the §16.14 error) -> /tmp/rwm-s.log.
# The --client is the bulk SENDER; the per-path anchor, pacer, depth budget, and the
# coded-emission counters all live here -> /tmp/rwm-c.log.  Sender-side DIAG (btlbw,
# dbud, cod, eff_pace, ANCHOR ...) MUST be scraped from /tmp/rwm-c.log.
ip netns exec "$NS_SRV" env $TENV "$BIN" perf --server --bind "$SRV_BIND" \
    --window-reliable $GEN_FLAG $OOO_FLAG $EXTRA --protocol-hint "$HINT" >/tmp/rwm-s.log 2>&1 &
SRV_PID=$!
aw_kv srv_pid "$SRV_PID"

SRV_WAITS=0
SRV_BOUND=0
for _ in $(seq 1 20); do
    if ip netns exec "$NS_SRV" ss -uln 2>/dev/null | grep -q ':7000'; then SRV_BOUND=1; break; fi
    SRV_WAITS=$((SRV_WAITS + 1))
    sleep 0.3
done
# THE SECOND PRE-TRANSFER CAUSE. The loop above has always been allowed to fall
# through silently after 6 s — a server that never bound produced exactly the
# same trace as one that bound instantly. `srv_bound=0` with an empty server log
# is "the process never started" (`ip netns exec` failed, or the binary died
# before `net::run_impl`'s `[GATES]` echo); `srv_bound=0` with a populated log
# is a bind failure the log itself explains.
aw_kv srv_bound "$SRV_BOUND"
aw_kv srv_waits "$SRV_WAITS"
kill -0 "$SRV_PID" 2>/dev/null && aw_kv srv_alive 1 || aw_kv srv_alive 0
if [ "$SRV_BOUND" -eq 0 ]; then
    aw_cause srv_bind "no :7000 in $SRV_WAITS x 0.3 s"
    aw_logs srv_bind
    aw_state srv_bind
fi
sleep 1

echo "--- RWM-C perf mode=$MODE hint=$HINT A=$SCENA B=$SCENB ooo=${RWM_OOO:-0} extra='$EXTRA' T=${PLACE_T:-default} ($BYTES x $RUNS) start=$(date +%T)"
# CPU accounting (goal-gate "Decode-CPU Ceiling"): the CLIENT (bulk sender /
# encoder) is wrapped in /usr/bin/time -v; the SERVER's (receiver / decoder)
# cumulative CPU is read from /proc/<pid>/stat right after the transfer, before
# teardown.  Reported as CPUCLI/CPUSRV seconds so utilization = cpu/elapsed.
rm -f /tmp/rwm-cli-time
# goal-gate "Latency Lever" — THE LOADED DELIVERED-LATENCY PROBE (RWM_LATPROBE=1),
# REPAIRED PER LEG for goal-gate "Latency Truth — PRE-REGISTRATION".
#
# The score for a latency control is what a *different* flow experiences while
# the bulk transfer is running. The engine's own `rtt=`/`rtp` gauges cannot
# supply that: they are the sender's estimate of its OWN path, produced by the
# code under test, on a flow whose pacing the mechanism changes. An
# independent ICMP flow sharing the SAME shaped qdisc is not — it is delivered
# round-trip time, measured by the kernel, identical in both arms, and it is
# exactly the standing queue a latency control is claimed to remove.
#
# WHAT THE TWO GAUGES MEASURE, WRITTEN DOWN HERE BECAUSE AN ADJUDICATION TURNS
# ON IT (era battery §4's unresolved sign disagreement):
#
#   `q_p50`  median(max(0, rtt - rtp)) computed BY THE CODE UNDER TEST from the
#            sender's OWN estimate of its OWN path. The engine's self-reported
#            standing queue. NOT delivered latency, and never was.
#   `ping_*` delivered RTT for an unrelated flow, measured by the KERNEL,
#            through the WHOLE shaped path — netem's fixed delay, its jitter,
#            its rate serialization, ITS queue, and our own bytes queued ahead
#            of the probe.
#
# DIFFERENT QUANTITIES. They may legitimately move in OPPOSITE directions (the
# engine drains its own queue while pushing more bytes into the shaped one).
# Neither is promoted here; both are recorded, never averaged, and the question
# of which one a latency CLAIM is entitled to is the battery's, not this file's.
#
# ── THREE DEFECTS THIS BLOCK CARRIED THROUGH THE ERA BATTERY ─────────────
#   1. ONE LEG OF TWO. It pinged `10.77.0.2` — path A — on EVERY topology,
#      including the ASYMMETRIC duals (`c8` = c2/c3). The arms load the legs
#      DIFFERENTLY, so a scheduler that shifts work to leg B empties the queue
#      the probe watched and fills the one it did not. Now: ONE PROBE PER LEG,
#      count derived from `CLI_LEGS` (so a quad gets four), addresses from the
#      SAME 10.(77+i).0.2 stride the bind lists use — one definition, so the
#      probe set cannot drift from the topology.
#   2. SIGTERM ATE THE LOSS ACCOUNTING. The reaper sent plain `kill`. `iputils`
#      `ping` installs `sigexit` on SIGINT and SIGALRM ONLY, so SIGTERM took the
#      default action and the process died WITHOUT the
#      `N packets transmitted, M received` summary that `era_parse.py` reads for
#      `ping_tx`/`ping_rx`/`ping_loss`. Those three columns were therefore None
#      on all 204 era invocations. Now: `kill -INT`, then a bounded wait for the
#      summary to land before the file is read.
#   3. LOSS CENSORS THE TAIL, IN THE FLATTERING DIRECTION. A lost probe never
#      produces a `time=` line. `topo_dual.sh` shapes the DATA direction with
#      `netem loss gemodel`, so at `c8` leg A drops 1.3/51.3 = 2.53 % and leg B
#      2/42 = 4.76 % of probes IN BURSTS, plus the loaded qdisc's tail drops.
#      Every censored sample is drawn from exactly the worst states, so a
#      percentile over the survivors is biased LOW. `latt_probe.py` computes the
#      censoring fraction and prints it BESIDE EVERY PERCENTILE.
#
# It must run HERE because the namespaces exist only for this script's
# lifetime. 20 probes/s per leg, backgrounded before the transfer starts and
# reaped after it ends; raw RTTs land in /tmp/rwm-ping-<i>.txt. Default OFF, so
# every existing driver is unchanged.
#
# BACKWARD COMPATIBILITY, STATED: /tmp/rwm-ping.txt is still written, as leg 0's
# file, because every existing caller passes that path to its parser. Its
# CONTENT is byte-identical to what this block always produced (same interval,
# same wait, same `-D`, same peer) — so a legacy column keeps its definition and
# the NEW per-leg columns are additive rather than a redefinition.
#
# Cost accounting, stated rather than assumed: 20 pkt/s of 84 B is 13 kbit/s,
# 1.3e-4 of a 100 Mbit cell — below the resolution of every goodput number
# here, and it is present in EVERY arm and now on EVERY leg, so it cannot
# favour one.
PING_PIDS=()
PING_FILES=()
if [[ "${RWM_LATPROBE:-0}" != "0" ]]; then
    rm -f /tmp/rwm-ping.txt
    for ((li = 0; li < ${#CLI_LEGS[@]}; li++)); do
        PF="/tmp/rwm-ping-$li.txt"
        rm -f "$PF"
        # NOT -q: the per-packet `time=<ms>` lines ARE the measurement; the
        # summary line only carries min/avg/max/mdev, and a tail percentile is
        # the whole point of a bufferbloat probe.
        ip netns exec "$NS_CLI" ping -i 0.05 -W 2 -D "10.$((77 + li)).0.2" > "$PF" 2>&1 &
        PP=$!
        PING_PIDS+=("$PP")
        PING_FILES+=("$PF")
        disown "$PP" 2>/dev/null || true
    done
fi
timeout 700 ip netns exec "$NS_CLI" /usr/bin/time -v -o /tmp/rwm-cli-time env $TENV "$BIN" perf --client \
    --peer "$PEERS" --bind "$CLI_BIND" \
    --window-reliable $GEN_FLAG $OOO_FLAG $EXTRA --protocol-hint "$HINT" \
    --bytes "$BYTES" --runs "$RUNS" 2>&1 | tee /tmp/rwm-c.log \
    | grep -E "summary|warmup|dnf|PFRAC" | tail -8
# THE THIRD PRE-TRANSFER CAUSE, and the `|| echo` it replaces is REPRODUCED
# EXACTLY below rather than removed. `PIPESTATUS[0]` is the CLIENT's own status
# (127 = `ip netns exec` could not exec at all, 124 = the 700 s `timeout` fired,
# anything else = the binary's exit) and it was previously unreadable: with
# `pipefail` on, a `grep` that matched nothing produced the same failed pipeline
# as a binary that never ran, and the `||` arm printed the same `dnf` marker for
# both. The array must be copied on the FIRST line after the pipeline — any
# other command in between overwrites it.
CLI_PIPE=("${PIPESTATUS[@]}")
CLI_RC="${CLI_PIPE[0]}"
CLI_ST=0
for _s in "${CLI_PIPE[@]}"; do [ "$_s" -ne 0 ] && CLI_ST="$_s"; done
[ "$CLI_ST" -ne 0 ] && echo "{\"dnf\":true,\"mode\":\"$MODE\"}"
# Recorded on EVERY invocation, so it is a column with a control and not only an
# abort field.
aw_kv cli_rc "$CLI_RC"
aw_kv cli_pipe "${CLI_PIPE[*]}"
# Reap the loaded-latency probes BEFORE the qdisc counters below, so their own
# packets are inside the tc totals every arm is measured on. ALL legs are reaped
# before ANY counter is read, for the same reason.
#
# `-INT`, NOT the default SIGTERM: `iputils` `ping` installs its `sigexit`
# statistics handler on SIGINT and SIGALRM only, so a SIGTERM'd probe dies
# WITHOUT the `N packets transmitted, M received` line — which is the ONLY
# count that includes probes lost after the last reply, i.e. exactly the
# consecutive-drop tail a bufferbloat probe exists to catch. Losing it is how
# the era battery's loss columns came out None on all 204 invocations.
if [[ "${#PING_PIDS[@]}" -gt 0 ]]; then
    # SIGINT to every leg FIRST, so all the probes stop at the same moment and
    # none of them keeps sending while another leg's summary is being waited on
    # — their packets would land in the tc counters unevenly across legs.
    for _pp in "${PING_PIDS[@]}"; do
        kill -INT "$_pp" 2>/dev/null || true
    done
    # The summary is written BY THE HANDLER, so the file is not complete the
    # instant the signal is delivered: poll for it, with a hard bound.
    #
    # AND A FALLBACK, because the SIGINT path has one way to fail that is worth
    # defending against. A shell sets SIGINT to SIG_IGN for jobs started with
    # `&` when job control is off, and a program that installs its handler with
    # the `if (signal(...) != SIG_IGN)` idiom would then never install one.
    # `iputils` `ping` uses `sigaction` unconditionally so it DOES catch it —
    # but `sigexit` is installed on SIGALRM as well, and SIGALRM is not subject
    # to that rule at all. So: INT, then ALRM if no summary appeared, then TERM
    # to guarantee the process is gone. Worst case ~2 s per leg AFTER the
    # transfer has ended; zero in the healthy case, where the first poll hits.
    _pi=0
    for _pf in "${PING_FILES[@]}"; do
        for _w in 1 2 3 4 5 6 7 8 9 10; do
            grep -q "packets transmitted" "$_pf" 2>/dev/null && break
            sleep 0.1
        done
        if ! grep -q "packets transmitted" "$_pf" 2>/dev/null; then
            kill -ALRM "${PING_PIDS[$_pi]}" 2>/dev/null || true
            for _w in 1 2 3 4 5 6 7 8 9 10; do
                grep -q "packets transmitted" "$_pf" 2>/dev/null && break
                sleep 0.1
            done
        fi
        kill -TERM "${PING_PIDS[$_pi]}" 2>/dev/null || true
        _pi=$((_pi + 1))
    done
    # Belt-and-braces for a probe whose pid was lost. The pattern is now the
    # LEG-GENERAL one: the old hard-coded `-D 10.77.0.2` would have left legs
    # 1..3 alive, still sending into the tc counters the NEXT arm is measured on.
    pkill -f "ping -i 0.05 -W 2 -D 10\.[0-9]*\.0\.2" 2>/dev/null || true
    # THE READOUT, WITH ITS CENSORING. One line per leg, every percentile
    # carrying its censoring fraction and its scoreability — a percentile
    # printed without one is the defect this repair closes.
    python3 ./latt_probe.py "${PING_FILES[@]}" 2>/dev/null | sed 's/^/    /' || true
    echo "    LATPROBE: ${#PING_FILES[@]} leg(s) $(for _pf in "${PING_FILES[@]}"; do printf '%s=%s ' "$_pf" "$(grep -c 'time=' "$_pf" 2>/dev/null || echo 0)"; done)replies"
    # Legacy path, written LAST and only as a copy: every existing caller passes
    # /tmp/rwm-ping.txt to its parser and must keep reading exactly leg A.
    cp "${PING_FILES[0]}" /tmp/rwm-ping.txt 2>/dev/null || true
fi
SRV_TICKS=0
for P in $(pgrep -x raptorpath); do
    T=$(awk '{print $14+$15}' /proc/$P/stat 2>/dev/null || echo 0)
    SRV_TICKS=$((SRV_TICKS + T))
done
HZ=$(getconf CLK_TCK)
CLI_U=$(grep -oP 'User time \(seconds\): \K[0-9.]+' /tmp/rwm-cli-time 2>/dev/null || echo 0)
CLI_S=$(grep -oP 'System time \(seconds\): \K[0-9.]+' /tmp/rwm-cli-time 2>/dev/null || echo 0)
echo "    CPU: CPUSRV=$(awk "BEGIN{printf \"%.2f\", $SRV_TICKS/$HZ}")s CPUCLI=$(awk "BEGIN{printf \"%.2f\", $CLI_U+$CLI_S}")s (srv=decoder cli=sender; whole-invocation incl warmup)"
echo "    done $(date +%T)"
echo "--- server log tail:"; sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-s.log | tail -3

# diag/lossy-residual (goal-gate "Lossy-Single Residual"): wire-truth qdisc
# counters BEFORE teardown — bytes/pkts that passed netem per direction plus
# its GE drops (the loss realization). Read-only; whole-invocation totals
# (warm-up object is 64 B — negligible). cli*=data direction, srv*=acks.
for DEV in "${CLI_LEGS[@]}"; do
    ST=$(ip netns exec "$NS_CLI" tc -s qdisc show dev "$DEV" 2>/dev/null | tr '\n' ' ') \
        && [[ -n "$ST" ]] && echo "    QDISC $DEV: $ST"
done
for DEV in "${SRV_LEGS[@]}"; do
    ST=$(ip netns exec "$NS_SRV" tc -s qdisc show dev "$DEV" 2>/dev/null | tr '\n' ' ') \
        && [[ -n "$ST" ]] && echo "    QDISC $DEV: $ST"
done

# goal-gate "Latency Lever", instrument 1 — TC COUNTERS ON EVERY CELL.
#
# The three-term battery captured tc for 2 of its 9 cells, and its central
# negative result ("the store was occupied to the new limit and throughput
# did not follow") needed exactly one number to be readable: the shaped
# link's utilisation. The flattened `QDISC` lines above ALREADY carry it —
# and `tt_battery.sh`'s grep filter threw them away, and their one-line
# form is not what any parser here reads.
#
# The capture MUST happen inside this script: `trap cleanup EXIT` above
# destroys both namespaces the instant this process returns, so by the time
# a caller regains control the qdiscs are gone. So write the sectioned form
# to a FIXED path and let the caller copy it under its own rep-unique name
# (the `adv_battery.sh` precedent). Callers that do not copy it pay nothing
# but a stale /tmp file.
#
# Banner names match `adv_cells.sh counters` so `bind_analyze.py`'s parser
# reads both without a second dialect. CLI1/SRV1 are NEW — dual cells (c7,
# c8) shape two veth pairs and only the first was ever nameable.
#
# CLI2/CLI3/SRV2/SRV3 arrive with the quad. The banner is `== CLI<n>` and the
# readers' device pattern is `(CLI\d|SRV\d)` (eppen_corr.py's `_QDEV`), which
# already admits a single digit 0-9 — so the quad's captures are read by the
# EXISTING seed audit with no parser change, and its per-leg seeds show up
# there as four distinct values instead of one repeated one. Checked, not
# assumed: the audit is what found the shared-seed defect, and a widened
# capture that its regex silently dropped would have retired the instrument
# at exactly the cell it was built for.
{
    for DEV in "${CLI_LEGS[@]}"; do
        ip netns exec "$NS_CLI" ip link show "$DEV" >/dev/null 2>&1 || continue
        echo "== ${DEV^^} (data-dir egress: netem or tbf+netem bottleneck)"
        ip netns exec "$NS_CLI" tc -s qdisc show dev "$DEV" 2>/dev/null || true
    done
    for DEV in "${SRV_LEGS[@]}"; do
        ip netns exec "$NS_SRV" ip link show "$DEV" >/dev/null 2>&1 || continue
        echo "== ${DEV^^} (ack-dir egress)"
        ip netns exec "$NS_SRV" tc -s qdisc show dev "$DEV" 2>/dev/null || true
    done
    echo "== SRV0-INGRESS (policer, when present)"
    ip netns exec "$NS_SRV" tc -s filter show dev srv0 parent ffff: 2>/dev/null || true
    # Wall duration of the shaped window, so utilisation is computable from
    # this file ALONE rather than joined against a RUNTIME line elsewhere.
    echo "== INVOCATION_S ${SECONDS}"
} > /tmp/rwm-q.txt 2>/dev/null || true
echo "    QCAP: /tmp/rwm-q.txt $(wc -l < /tmp/rwm-q.txt 2>/dev/null || echo 0) lines"

# ── THE WITNESS'S CLOSING READ, and it MUST happen here ──────────────────
# `trap cleanup EXIT` destroys both namespaces the instant this process returns,
# so a caller that discovers the missing `[GATES]` from the log files has no
# way left to ask what the netns/interface/socket state was when it happened.
# This is the same reason the tc capture above lives in this script.
#
# The residual cause `no_gates_unknown` is deliberately NOT a synonym for the
# abort: it is the record that all four instrumented steps reported success and
# the engine still never echoed. If the class turns out to be concentrated
# there, this witness has FALSIFIED its own four hypotheses and the next
# instrument is named rather than guessed.
aw_logs final
# `grep -c` PRINTS `0` and EXITS 1 on no match, so the idiom must be `|| true`
# and never `|| echo 0` — the latter yields the two-word string `0 0` and turns
# the test below into a shell error, which is precisely how a witness stops
# witnessing.
GC=$(grep -c '\[GATES\]' /tmp/rwm-c.log 2>/dev/null || true); GC="${GC:-0}"
GS=$(grep -c '\[GATES\]' /tmp/rwm-s.log 2>/dev/null || true); GS="${GS:-0}"
aw_kv gates_cli "$GC"
aw_kv gates_srv "$GS"
if [ "$GC" -eq 0 ] && [ "$GS" -eq 0 ]; then
    aw_cause no_gates_unknown "all instrumented steps reported OK; cli_rc=$CLI_RC srv_bound=$SRV_BOUND"
    aw_state no_gates
fi
aw_kv aw_finished "$(date -u +%FT%TZ)"

# --- HARD SANITY GUARD (feat/gen-on-rebaseline) -----------------------------------
# A measurement where the mechanism under test did not run must FAIL LOUDLY, not
# silently report a number.  When generation is requested (GEN_FLAG set, i.e.
# RWM_GEN!=0), assert that CODED symbols actually flowed on the SENDER.  The sender
# is the --client => /tmp/rwm-c.log (NOT the --server/receiver /tmp/rwm-s.log — the
# §16.14 wrong-log trap).  Coded count = max total_coded over the run's [PFRAC] lines.
if [[ -n "$GEN_FLAG" ]]; then
    CODED=$(sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log 2>/dev/null \
        | grep -oE 'total_coded=[0-9]+' | grep -oE '[0-9]+' | sort -n | tail -1)
    CODED="${CODED:-0}"
    if [[ "$CODED" -le 0 ]]; then
        aw_cause guard_cod0 "generation requested, total_coded=0 on the sender"
        echo "FATAL: generation requested but cod=0 (mechanism inert) -- NO coded symbols flowed on the sender (/tmp/rwm-c.log). The measured binary ran the coded path DEAD; the numbers above are INVALID. Check that --window-generation-coding is on the wire and RWM_GEN!=0." >&2
        exit 7
    fi
    echo "    GUARD OK: generation ACTIVE on the sender (total_coded=$CODED coded symbols flowed)"
fi
