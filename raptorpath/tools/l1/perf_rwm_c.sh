#!/bin/bash
# `raptorpath perf` (rp-native objects) over the reliable sliding-window
# pipeline, single, dual or quad path, with an optional out-of-order object
# delivery toggle (the H->inf corner, paper §4.11).
#
#   sudo bash perf_rwm_c.sh <scenA> <scenB> <hint> <bytes> <runs> <dual|single|quad> [T]
#
#   env RWM_OOO=1     -> add --window-out-of-order (H->inf, decode-on-total)
#   env RWM_EXTRA=".." -> extra CLI args appended to server+client (raise-r arm)
#   env RWM_PLACE_T=.. -> placement-temperature override (via 7th arg too)
#   env RWM_C_PIPELINE=window (the only value) -> `block` is refused: the
#                        block pipeline was deleted (ADR-0069, executed)
#
#   C7 = c2 c2   C8 = c2 c3
#
# Quad mode runs four veth legs via topo_quad.sh; the two scenario arguments
# are the two leg classes, each used for two legs: `<scenA> <scenA> <scenB>
# <scenB>`. So
#
#   C9  = c2 c2 ... quad   ->  c2 c2 c2 c2   the symmetric quad
#   C9H = c2 c3 ... quad   ->  c2 c2 c3 c3   the heterogeneous quad
#
# Only 2 + 2 geometries are expressible here; four distinct classes or a
# 3 + 1 split would need another positional argument (topo_quad.sh itself
# takes four independent scenarios).
set -uo pipefail
cd "$(dirname "$0")"
source ./lib.sh
# The abort-cause witness. Recorders only: every capture below preserves the
# exit code it observed and changes no control flow. See abort_witness.sh for
# why "no [GATES] on either endpoint" narrows the cause to the four
# pre-transfer steps instrumented here.
source ./abort_witness.sh
# The binary is overridable so a battery can run a second binary as a baseline
# arm; `$BIN` is echoed into the witness to prove which binary an invocation
# ran.
BIN="${RWM_BIN:-/home/vibe/raptorpath/target/release/raptorpath}"
SCENA="${1:?scenA}"; SCENB="${2:?scenB}"; HINT="${3:-bulk}"
BYTES="${4:-1800000}"; RUNS="${5:-10}"; MODE="${6:-dual}"; PLACE_T="${7:-}"

# Gate forwarding: one shared list in lib.sh, so every set RWM_* gate reaches
# both endpoints explicitly (docs/measurement-discipline.md rule 15).
TENV="$(rwm_forward_env)"
[[ -n "$PLACE_T" ]] && TENV="$TENV RWM_PLACE_T=$PLACE_T"

# Name collision: the binary reads RWM_GEN as the generation size G (gates.rs,
# default 384, `.max(1)`), while this harness uses it as the on/off gate for
# --window-generation-coding: RWM_GEN=0 -> plain window control, unset/1 ->
# generation on at the binary's default G.
#
# The sentinels 0 and 1 must not reach the binary (=1 would set a 1-symbol
# generation). The binary inherits this script's whole environment, so the
# only way to withhold a var is to `unset` it here; a real generation size
# (>=2) is forwarded normally by rwm_forward_env.
GEN_GATE="${RWM_GEN:-1}"
if [[ "${RWM_GEN:-}" == "0" || "${RWM_GEN:-}" == "1" ]]; then
    unset RWM_GEN
    TENV="$(rwm_forward_env)"
    [[ -n "$PLACE_T" ]] && TENV="$TENV RWM_PLACE_T=$PLACE_T"
fi

OOO_FLAG=""
[[ "${RWM_OOO:-0}" == "1" ]] && OOO_FLAG="--window-out-of-order"
EXTRA="${RWM_EXTRA:-}"

# Generation defaults on here. The coded/generation pipeline is enabled only by
# the --window-generation-coding CLI flag (net/mod.rs gates window_generation on
#   window_reliable && (window_generation_coding || window_systematic_repair));
# RWM_GEN_R/RWM_RATE_SAMPLE only configure it. Set RWM_GEN=0 for the
# plain-window control. --window-reliable is kept (generation requires it).
GEN_FLAG="--window-generation-coding"
# GEN_GATE, not RWM_GEN: the sentinel branch above `unset`s RWM_GEN so it cannot
# reach the binary as a 1-symbol generation size, so the GATE meaning must be
# read from the saved copy.
[[ "$GEN_GATE" == "0" ]] && GEN_FLAG=""

# The pipeline arm. Not named RWM_PIPELINE: that is an engine gate and the
# binary inherits this script's environment. The block pipeline is deleted
# (ADR-0069, executed); its re-test is done and lives in history, so `block`
# is refused rather than silently run as the window. `window` passes
# --window-reliable (ρ = 1, the Bulk/Auto default — explicit so the Realtime
# hint is retain-until-acked here too). Echoed on the `--- RWM-C perf` line.
PIPELINE="${RWM_C_PIPELINE:-window}"
case "$PIPELINE" in
    window) WR_FLAG="--window-reliable" ;;
    block)
        echo "RWM_C_PIPELINE=block: the block pipeline was deleted (ADR-0069, executed); there is no block arm to run" >&2
        exit 2
        ;;
    *) echo "unknown RWM_C_PIPELINE '$PIPELINE' (want window)" >&2; exit 2 ;;
esac
# Force the cumulative coded-emission counter on so the sanity guard (below)
# can assert cod>0 on the sender.  RWM_PFRAC makes run_window_sender print
# "[PFRAC] ... total_coded=N ..." every 500 ms (generation-gated, cheap).
if [[ -n "$GEN_FLAG" && -z "${RWM_PFRAC:-}" ]]; then
    TENV="$TENV RWM_PFRAC=1"
fi

# The device lists this invocation's topology owns. Set with the mode, read by
# the qdisc captures at the bottom — one definition, so a capture cannot keep
# reading two legs after the topology grows to four.
case "$MODE" in
    quad)   CLI_LEGS=(cli0 cli1 cli2 cli3); SRV_LEGS=(srv0 srv1 srv2 srv3) ;;
    dual)   CLI_LEGS=(cli0 cli1);           SRV_LEGS=(srv0 srv1) ;;
    single) CLI_LEGS=(cli0);                SRV_LEGS=(srv0) ;;
    *) echo "unknown mode '$MODE' (want single|dual|quad)" >&2; exit 2 ;;
esac
TOPO=./topo_dual.sh
[[ "$MODE" == "quad" ]] && TOPO=./topo_quad.sh

cleanup() {
    # TERM + 3 s grace + KILL (lib.sh): the receiver's `final=1` exit flush
    # rides on the graceful path, and the server log is read AFTER this.
    stop_raptorpath
    # BOTH topologies are torn down regardless of this invocation's mode: they
    # share the rp-cli/rp-srv namespaces, so a quad left behind by a crashed
    # run would otherwise be inherited by the next dual run as a four-legged
    # cell wearing a two-legged cell's name. `down` is idempotent and deletes
    # the namespaces, so the second call is a no-op.
    bash ./topo_dual.sh down >/dev/null 2>&1 || true
    bash ./topo_quad.sh down >/dev/null 2>&1 || true
}
trap cleanup EXIT
# Arm the witness before the first thing that can fail — `cleanup` itself,
# whose `pkill` opens the SIGTERM race the `BUSY` pre-check below can lose.
aw_begin "perf_rwm_c $SCENA/$SCENB/$MODE $BYTES"
aw_kv bin "$BIN"
cleanup
# The teardown race, measured on every invocation (not only on aborts, so the
# column has a control). `cleanup` has just sent SIGTERM; the `pgrep` below
# aborts the invocation if anything is still alive. SIGTERM is not synchronous
# and shutdown duration differs per arm. The probe is instantaneous by
# construction: a witness that slept here would lower the abort rate it exists
# to explain.
aw_drain_probe
# The tc capture below writes a FIXED path, so a run that aborts before
# reaching it would leave the PREVIOUS invocation's counters there for the
# caller to copy under this cell's name. Silently attributing one cell's
# wire truth to another is worse than having no capture, so clear it first:
# an absent file is then an unambiguous "this invocation produced none".
rm -f /tmp/rwm-q.txt

if pgrep -x raptorpath >/dev/null 2>&1; then
    echo "BUSY: raptorpath already running -- aborting" >&2
    # The first of the four pre-transfer abort causes, and the only one that
    # produces no log file on either endpoint. Recorded after the decision.
    aw_cause busy_precheck "pgrep hit after cleanup's SIGTERM"
    aw_state busy
    aw_drain_watch
    exit 3
fi

# The witness records `topo*.sh up`'s exit code and stderr; control flow does
# not depend on it (`aw_step` re-returns the code and nothing here reads it),
# so the witness explains the abort class without moving it.
#
# SEED is a per-leg spec (see lib.sh): a bare `42` derives 42/1042/2042/3042
# and gives every leg its own netem realization. It is passed through verbatim
# so a caller can still pin the legs equal (`SEED=42,42`).
if [[ "$MODE" == "quad" ]]; then
    aw_step topo_up bash "$TOPO" up "$SCENA" "$SCENA" "$SCENB" "$SCENB" \
        --seed "${SEED:-42}"
else
    aw_step topo_up bash "$TOPO" up "$SCENA" "$SCENB" --seed "${SEED:-42}"
fi
aw_state post_topo
# The receiver netns's kernel UDP receive-buffer drops, baselined before the
# server starts: the one local-loss term no engine token counts (read again
# at run end for the `[TRUTH]` lines' `rcvbuf_drops=`).
RCVBUF0=$(udp_snmp_field "$NS_SRV" RcvbufErrors)

# Bind/peer lists are built from the leg list, not written out per mode: the
# addressing stride is 10.(77+i).0.x, so the address set and the device set
# cannot drift apart when a leg is added.
SRV_BIND=""; PEERS=""; CLI_BIND=""
for ((li = 0; li < ${#CLI_LEGS[@]}; li++)); do
    SRV_BIND="${SRV_BIND}${SRV_BIND:+,}10.$((77 + li)).0.2:7000"
    CLI_BIND="${CLI_BIND}${CLI_BIND:+,}10.$((77 + li)).0.1:0"
done
PEERS="$SRV_BIND"

# Log sources: the --server is the perf receiver of the bulk transfer (its
# reverse sender loop places almost no source or coded symbols, so its
# sender-side counters are meaningless) -> /tmp/rwm-s.log. The --client is the
# bulk sender; the per-path anchor, pacer, depth budget and coded-emission
# counters live there -> /tmp/rwm-c.log. Sender-side DIAG (btlbw, dbud, cod,
# eff_pace, ANCHOR ...) must be scraped from /tmp/rwm-c.log.
ip netns exec "$NS_SRV" env $TENV "$BIN" perf --server --bind "$SRV_BIND" \
    $WR_FLAG $GEN_FLAG $OOO_FLAG $EXTRA --protocol-hint "$HINT" >/tmp/rwm-s.log 2>&1 &
SRV_PID=$!
aw_kv srv_pid "$SRV_PID"

SRV_WAITS=0
SRV_BOUND=0
for _ in $(seq 1 20); do
    # `grep -c`, not `grep -q`: under pipefail an early-exiting `grep -q` can
    # SIGPIPE `ss`, and the failed pipeline would read a BOUND server as unbound.
    if [ "$(ip netns exec "$NS_SRV" ss -uln 2>/dev/null | grep -c ':7000')" -gt 0 ]; then SRV_BOUND=1; break; fi
    SRV_WAITS=$((SRV_WAITS + 1))
    sleep 0.3
done
# The second pre-transfer cause. The loop above falls through after 6 s.
# `srv_bound=0` with an empty server log is "the process never started"
# (`ip netns exec` failed, or the binary died before `net::run_impl`'s
# `[GATES]` echo); `srv_bound=0` with a populated log is a bind failure the
# log itself explains.
aw_kv srv_bound "$SRV_BOUND"
aw_kv srv_waits "$SRV_WAITS"
kill -0 "$SRV_PID" 2>/dev/null && aw_kv srv_alive 1 || aw_kv srv_alive 0
if [ "$SRV_BOUND" -eq 0 ]; then
    aw_cause srv_bind "no :7000 in $SRV_WAITS x 0.3 s"
    aw_logs srv_bind
    aw_state srv_bind
fi
sleep 1

echo "--- RWM-C perf pipeline=$PIPELINE mode=$MODE hint=$HINT A=$SCENA B=$SCENB ooo=${RWM_OOO:-0} extra='$EXTRA' T=${PLACE_T:-default} ($BYTES x $RUNS) start=$(date +%T)"
# CPU accounting: the client (bulk sender / encoder) is wrapped in
# /usr/bin/time -v; the server's (receiver / decoder)
# cumulative CPU is read from /proc/<pid>/stat right after the transfer, before
# teardown.  Reported as CPUCLI/CPUSRV seconds so utilization = cpu/elapsed.
rm -f /tmp/rwm-cli-time
# The loaded delivered-latency probe (RWM_LATPROBE=1), one per leg.
#
# The score for a latency control is what a *different* flow experiences while
# the bulk transfer runs. The engine's own `rtt=`/`rtp` gauges are the
# sender's estimate of its own path, produced by the code under test. An
# independent ICMP flow sharing the same shaped qdisc is delivered round-trip
# time, measured by the kernel, identical in both arms.
#
# The two gauges measure different quantities and may move in opposite
# directions (the engine can drain its own queue while pushing more bytes into
# the shaped one); both are recorded, never averaged:
#
#   `q_p50`  median(max(0, rtt - rtp)) computed by the code under test from the
#            sender's own estimate of its own path: self-reported standing
#            queue, not delivered latency.
#   `ping_*` delivered RTT for an unrelated flow, measured by the kernel,
#            through the whole shaped path — netem's fixed delay, its jitter,
#            its rate serialization, its queue, and our own bytes queued ahead
#            of the probe.
#
# Design constraints:
#   1. One probe per leg, count derived from `CLI_LEGS`, addresses from the
#      same 10.(77+i).0.2 stride the bind lists use. On asymmetric duals the
#      arms load the legs differently, so one leg's probe is not enough.
#   2. Reaped with SIGINT, not SIGTERM: `iputils` `ping` prints its
#      `N packets transmitted, M received` summary only from its SIGINT/SIGALRM
#      handler, and that summary is what the parsers read for loss.
#   3. Loss censors the tail downward: a lost probe never produces a `time=`
#      line, and netem drops in bursts in exactly the worst states, so a
#      percentile over the survivors is biased low. `latt_probe.py` prints the
#      censoring fraction beside every percentile.
#
# It must run here because the namespaces exist only for this script's
# lifetime. 20 probes/s per leg, backgrounded before the transfer starts and
# reaped after it ends; raw RTTs land in /tmp/rwm-ping-<i>.txt. Default off.
# /tmp/rwm-ping.txt is also written, as leg 0's file, for callers that parse
# that path.
#
# Cost: 20 pkt/s of 84 B is 13 kbit/s, 1.3e-4 of a 100 Mbit cell, present in
# every arm and on every leg.
PING_PIDS=()
PING_FILES=()
if [[ "${RWM_LATPROBE:-0}" != "0" ]]; then
    rm -f /tmp/rwm-ping.txt
    for ((li = 0; li < ${#CLI_LEGS[@]}; li++)); do
        PF="/tmp/rwm-ping-$li.txt"
        rm -f "$PF"
        # Not -q: the per-packet `time=<ms>` lines are the measurement; the
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
    $WR_FLAG $GEN_FLAG $OOO_FLAG $EXTRA --protocol-hint "$HINT" \
    --bytes "$BYTES" --runs "$RUNS" 2>&1 | tee /tmp/rwm-c.log \
    | grep -E "summary|warmup|dnf|PFRAC" | tail -8
# The third pre-transfer cause. `PIPESTATUS[0]` is the client's own status
# (127 = `ip netns exec` could not exec at all, 124 = the 700 s `timeout`
# fired, anything else = the binary's exit); under `pipefail` a `grep` that
# matched nothing would otherwise look the same as a binary that never ran.
# The array must be copied on the first line after the pipeline — any other
# command in between overwrites it.
CLI_PIPE=("${PIPESTATUS[@]}")
CLI_RC="${CLI_PIPE[0]}"
CLI_ST=0
for _s in "${CLI_PIPE[@]}"; do [ "$_s" -ne 0 ] && CLI_ST="$_s"; done
[ "$CLI_ST" -ne 0 ] && echo "{\"dnf\":true,\"mode\":\"$MODE\"}"
# Recorded on every invocation, so it is a column with a control and not only
# an abort field.
aw_kv cli_rc "$CLI_RC"
aw_kv cli_pipe "${CLI_PIPE[*]}"
# Reap the loaded-latency probes before the qdisc counters below, so their own
# packets are inside the tc totals every arm is measured on. All legs are
# reaped before any counter is read, for the same reason.
#
# `-INT`, not the default SIGTERM: `iputils` `ping` installs its `sigexit`
# statistics handler on SIGINT and SIGALRM only, so a SIGTERM'd probe dies
# without the `N packets transmitted, M received` line — the only count that
# includes probes lost after the last reply.
if [[ "${#PING_PIDS[@]}" -gt 0 ]]; then
    # SIGINT to every leg first, so all the probes stop at the same moment and
    # none of them keeps sending while another leg's summary is being waited on
    # — their packets would land in the tc counters unevenly across legs.
    for _pp in "${PING_PIDS[@]}"; do
        kill -INT "$_pp" 2>/dev/null || true
    done
    # The summary is written by the handler, so the file is not complete the
    # instant the signal is delivered: poll for it, with a hard bound.
    #
    # Fallback: a shell sets SIGINT to SIG_IGN for `&` jobs when job control is
    # off, and a program using the `if (signal(...) != SIG_IGN)` idiom would
    # never install its handler. SIGALRM is not subject to that rule and
    # `sigexit` handles it too. So: INT, then ALRM if no summary appeared, then
    # TERM to guarantee the process is gone. Worst case ~2 s per leg after the
    # transfer; zero in the healthy case.
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
    # Belt-and-braces for a probe whose pid was lost; the pattern matches every
    # leg, so no probe keeps sending into the next arm's tc counters.
    pkill -f "ping -i 0.05 -W 2 -D 10\.[0-9]*\.0\.2" 2>/dev/null || true
    # The readout: one line per leg, every percentile carrying its censoring
    # fraction and its scoreability.
    python3 ./latt_probe.py "${PING_FILES[@]}" 2>/dev/null | sed 's/^/    /' || true
    echo "    LATPROBE: ${#PING_FILES[@]} leg(s) $(for _pf in "${PING_FILES[@]}"; do printf '%s=%s ' "$_pf" "$(grep -c 'time=' "$_pf" 2>/dev/null || echo 0)"; done)replies"
    # /tmp/rwm-ping.txt is leg 0's file, written last as a copy, for callers
    # that parse that path.
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

# netem's qdisc counters before teardown: bytes/pkts that passed netem per
# direction plus its drops. Read-only; whole-invocation totals (warm-up object
# is 64 B, negligible). cli*=data direction, srv*=acks.
#
# These are NOT loss truth. `Sent … pkt` counts datagrams (GSO segments) but
# `dropped` counts skbs, each of which may carry several datagrams, so
# `dropped / Sent` understates datagram loss by the GSO factor. The truth is
# the `[TRUTH]` lines below. `qdisc_stats_netem` leaves out the truth
# counter's clsact block so these lines read exactly as before it existed.
for DEV in "${CLI_LEGS[@]}"; do
    ST=$(qdisc_stats_netem "$NS_CLI" "$DEV" | tr '\n' ' ') \
        && [[ -n "$ST" ]] && echo "    QDISC $DEV: $ST"
done
for DEV in "${SRV_LEGS[@]}"; do
    ST=$(qdisc_stats_netem "$NS_SRV" "$DEV" | tr '\n' ' ') \
        && [[ -n "$ST" ]] && echo "    QDISC $DEV: $ST"
done

# Loss truth, per datagram, per data-direction leg (lib.sh `truth_line`): the
# egress counter the topology installed ahead of netem, against what netem
# passed. `leg=<i>` is the CLI_LEGS index, i.e. the engine's `p<i>` (the bind
# list is built in the same order). `gso` = egress datagrams per skb, the
# factor by which netem's own `dropped` understates datagram loss.
# `rcvbuf_drops` is the receiver netns's RcvbufErrors delta over the run
# (per netns, not per leg): datagrams the kernel dropped after the wire.
RCVBUF1=$(udp_snmp_field "$NS_SRV" RcvbufErrors)
RCVBUF_D="-"
[[ -n "${RCVBUF0:-}" && -n "$RCVBUF1" ]] && RCVBUF_D=$((RCVBUF1 - RCVBUF0))
TRUTH_LINES=""
for ((li = 0; li < ${#CLI_LEGS[@]}; li++)); do
    TL=$(truth_line "$NS_CLI" "${CLI_LEGS[$li]}" "$li" "$RCVBUF_D")
    TRUTH_LINES="${TRUTH_LINES}${TL}"$'\n'
    echo "    $TL"
done

# tc counters on every cell, in sectioned form, so the shaped link's
# utilisation is readable for every run.
#
# The capture must happen inside this script: `trap cleanup EXIT` destroys both
# namespaces the instant this process returns. So write to a fixed path and let
# the caller copy it under its own rep-unique name.
#
# Banner names match `adv_cells.sh counters` so one parser reads both. The
# banner is `== CLI<n>`/`== SRV<n>`, one per leg (up to four with the quad);
# readers match the device as `(CLI\d|SRV\d)`.
{
    for DEV in "${CLI_LEGS[@]}"; do
        ip netns exec "$NS_CLI" ip link show "$DEV" >/dev/null 2>&1 || continue
        echo "== ${DEV^^} (data-dir egress: netem or tbf+netem bottleneck)"
        qdisc_stats_netem "$NS_CLI" "$DEV" || true
    done
    for DEV in "${SRV_LEGS[@]}"; do
        ip netns exec "$NS_SRV" ip link show "$DEV" >/dev/null 2>&1 || continue
        echo "== ${DEV^^} (ack-dir egress)"
        qdisc_stats_netem "$NS_SRV" "$DEV" || true
    done
    echo "== SRV0-INGRESS (policer, when present)"
    ip netns exec "$NS_SRV" tc -s filter show dev srv0 parent ffff: 2>/dev/null || true
    # Wall duration of the shaped window, so utilisation is computable from
    # this file ALONE rather than joined against a RUNTIME line elsewhere.
    echo "== INVOCATION_S ${SECONDS}"
    # The per-datagram loss truth: the same `[TRUTH]` lines printed above (one
    # read of the counters), so a battery's copied `-q.txt` carries them.
    echo "== TRUTH (per-datagram loss per data leg; lib.sh truth_line)"
    printf '%s' "$TRUTH_LINES"
} > /tmp/rwm-q.txt 2>/dev/null || true
echo "    QCAP: /tmp/rwm-q.txt $(wc -l < /tmp/rwm-q.txt 2>/dev/null || echo 0) lines"

# The witness's closing read must happen here: `trap cleanup EXIT` destroys
# both namespaces the instant this process returns, so a caller has no way left
# to ask what the netns/interface/socket state was.
#
# The residual cause `no_gates_unknown` is not a synonym for the abort: it
# records that all four instrumented steps reported success and the engine
# still never echoed, i.e. none of the four hypotheses explains that run.
aw_logs final
# `grep -c` prints `0` and exits 1 on no match, so the idiom must be `|| true`
# and never `|| echo 0` — the latter yields the two-word string `0 0` and turns
# the test below into a shell error.
GC=$(grep -c '\[GATES\]' /tmp/rwm-c.log 2>/dev/null || true); GC="${GC:-0}"
GS=$(grep -c '\[GATES\]' /tmp/rwm-s.log 2>/dev/null || true); GS="${GS:-0}"
aw_kv gates_cli "$GC"
aw_kv gates_srv "$GS"
if [ "$GC" -eq 0 ] && [ "$GS" -eq 0 ]; then
    aw_cause no_gates_unknown "all instrumented steps reported OK; cli_rc=$CLI_RC srv_bound=$SRV_BOUND"
    aw_state no_gates
fi
aw_kv aw_finished "$(date -u +%FT%TZ)"

# --- Sanity guard ----------------------------------------------------------------
# A measurement where the mechanism under test did not run must fail loudly
# (docs/measurement-discipline.md rule 1). When generation is requested
# (GEN_FLAG set, i.e. RWM_GEN!=0), assert that coded symbols flowed on the
# sender — the --client, /tmp/rwm-c.log, not the receiver's /tmp/rwm-s.log.
# Coded count = max total_coded over the run's [PFRAC] lines.
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
