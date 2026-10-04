#!/bin/bash
# Realtime-vs-bulk tail comparison: bring the tunnel up once per arm, run N
# stream measurements through the same warm tunnel, and report the p99
# distribution (a single-run p99 is variance-dominated). Matrix:
# {realtime,bulk} x {400,1200}B at <cell>. Every arm is hard-timeout-bounded
# so nothing can wedge the matrix.
#   sudo bash tail_matrix.sh <cell> <reps>
#
# Named-arm mode: RWM_TM_ARMS="ship unified rlc ..." runs the realtime hint
# (unless the arm says otherwise) once per named arm x size; the arm table is
# at the bottom. Mechanism-liveness echoes are scraped from both endpoint logs
# per arm. SEED forwards to topo.sh (default 42). With RWM_TM_ARMS unset the
# plain {realtime,bulk} matrix runs.
set -uo pipefail
cd "$(dirname "$0")" || { echo "ABORT-CD $(dirname "$0")"; exit 3; }
source ./lib.sh
# lib.sh runs `set -euo pipefail` for its topology callers; this matrix runs
# per-arm abort tolerance (a failed arm is a BRINGUP_FAIL/NO_DATA line, never
# a matrix kill), so errexit is turned back off here.
set +e
BIN="${RWM_BIN:-/home/vibe/raptorpath/target/release/raptorpath}"
CELL="${1:-c2}"; REPS="${2:-5}"
SEED="${SEED:-42}"
# Message rate, stream duration and size list are overridable (e.g. the
# 50 msg/s x 30 s, 1200 B stream_bench shape). Defaults: 50 msg/s x 20 s,
# 400 and 1200 B.
TM_RATE="${RWM_TM_RATE:-50}"; TM_DUR="${RWM_TM_DUR:-20}"
TM_SIZES="${RWM_TM_SIZES:-400 1200}"
TM_TMO=$((TM_DUR + 10))
# The topology script is overridable (RWM_TM_TOPO=./adv_cells.sh) so the same
# matrix can run on an adversarial cell (`up <cell> [--seed N]` is shared by
# topo.sh and adv_cells.sh). Default: topo.sh.
TM_TOPO="${RWM_TM_TOPO:-./topo.sh}"

hard_cleanup() {
    stop_raptorpath   # TERM, 3 s grace, then KILL (lib.sh)
    pkill -f 'python3 ./transfer_bench.py' 2>/dev/null || true
    ip netns del "$NS_CLI" 2>/dev/null || true
    ip netns del "$NS_SRV" 2>/dev/null || true
}
# One EXIT handler: a second `trap ... EXIT` replaces the first. The rc is
# captured first and the handler does not `exit`, so the script's own status
# is unchanged.
on_exit() {
    local rc=$?
    hard_cleanup
    echo "EXIT rc=$rc ($(date +%T))"
}
trap on_exit EXIT

run_arm() { # hint size label armenv armflags -> one warm tunnel, REPS stream measurements
    local hint="$1" size="$2" label="${3:-$1}" armenv="${4:-}" armflags="${5:-}"
    echo "ARMENV $label ${size}B: hint=$hint env='${armenv:-<unset>}' flags='${armflags:-}' rate=$TM_RATE dur=$TM_DUR"
    # docs/measurement-discipline.md rule 7: a transient topo bringup failure
    # must fail this arm loudly (the ping probe below catches it), not kill
    # the whole matrix silently.
    # Per-rep-interleaved invocations cycle netns fast enough to hit
    # transient bringup collisions, so the whole bringup is retried up to 3
    # times, each attempt counted (BRINGUP_RETRY), before the arm is declared
    # BRINGUP_FAIL. The stream reps only run after a verified ping.
    local up=0 attempt
    for attempt in 1 2 3; do
        hard_cleanup; sleep 1
        bash "$TM_TOPO" up "$CELL" --seed "$SEED" >/dev/null 2>&1 || true
        # shellcheck disable=SC2086
        ip netns exec "$NS_SRV" env $(rwm_forward_env) $armenv "$BIN" run --server --bind 10.77.0.2:7000 \
            --tun-name rpsrv0 --tun-addr 10.99.0.2/24 --protocol-hint "$hint" $armflags \
            >/tmp/tm-s.log 2>&1 &
        sleep 2
        # shellcheck disable=SC2086
        ip netns exec "$NS_CLI" env $(rwm_forward_env) $armenv "$BIN" run --peer 10.77.0.2:7000 --bind 10.77.0.1:0 \
            --tun-name rpcli0 --tun-addr 10.99.0.1/24 --protocol-hint "$hint" $armflags \
            >/tmp/tm-c.log 2>&1 &
        for i in $(seq 1 20); do
            ip netns exec "$NS_CLI" ping -c1 -W1 10.99.0.2 >/dev/null 2>&1 && { up=1; break; }
            sleep 1
        done
        [[ $up -eq 1 ]] && break
        echo "  BRINGUP_RETRY $label ${size}B attempt=$attempt failed"
    done
    [[ $up -eq 0 ]] && { echo "ARM $label ${size}B: BRINGUP_FAIL"; hard_cleanup; return; }
    # Mechanism-liveness echoes (docs/measurement-discipline.md rule 1):
    # code-family selection + decoder machine, from both endpoints. Every
    # pipeline here must be no-match-safe (the rlc arm has no RWM_UNIFIED
    # echo, and an unguarded grep under set -e would kill the matrix).
    for lg in /tmp/tm-s.log /tmp/tm-c.log; do
        sed 's/\x1b\[[0-9;]*m//g' "$lg" 2>/dev/null \
            | grep -oE '(RWM_UNIFIED[^"]*|Realtime mode: auto-selecting streaming[^"]*|auto-selecting RLC windowed backend|unified span law ACTIVE[^"]*|unified overload shedding ACTIVE[^"]*|A\* send-rate anchor ACTIVE[^"]*|clock-gap estimator hygiene ACTIVE[^"]*|M\* peer-report RTT-feed suppression ACTIVE[^"]*|backend=[A-Za-z]+ sliding-window FEC mode|sliding-window FEC mode[^"]*|quinn congestion controller: BBR[^"]*|RWM_QUIC_CC=passthrough[^"]*|derived patience ACTIVE[^"]*|derived stall gauge ACTIVE[^"]*|estimator heavy-math cadence (ACTIVE|OFF)[^"]*|ack-merge ACTIVE[^"]*)' \
            | sort -u | sed "s|^|  ECHO $label ${size}B ${lg##*/}: |" || true
    done
    local p99s=() p50s=()
    for r in $(seq 1 "$REPS"); do
        : > /tmp/tm-srv.log
        ip netns exec "$NS_SRV" timeout "$TM_TMO" python3 ./transfer_bench.py stream-server \
            --bind 10.99.0.2 --port 9910 >/tmp/tm-srv.log 2>&1 &
        local spid=$!
        sleep 0.5
        timeout "$TM_TMO" ip netns exec "$NS_CLI" python3 ./transfer_bench.py stream-client \
            --host 10.99.0.2 --port 9910 --rate "$TM_RATE" --duration "$TM_DUR" --size "$size" \
            >/dev/null 2>&1 || true
        wait $spid 2>/dev/null || true
        local p99 p50 sline
        # no-summary-safe under lib.sh's set -e (a timed-out rep must be a
        # skipped datum, not a matrix kill)
        sline=$({ grep '"summary"' /tmp/tm-srv.log || true; } | tail -1)
        p99=$(echo "$sline" | sed -n 's/.*"p99_ms": \([0-9.]*\).*/\1/p')
        p50=$(echo "$sline" | sed -n 's/.*"p50_ms": \([0-9.]*\).*/\1/p')
        # Delivered count per rep: shedding must stay within the 1−ρ class
        # (paper §5.6); rate*dur msgs are sent per rep. p999/max are scraped
        # too.
        local cnt p999 pmax
        cnt=$(echo "$sline" | sed -n 's/.*"count": \([0-9]*\).*/\1/p')
        p999=$(echo "$sline" | sed -n 's/.*"p999_ms": \([0-9.]*\).*/\1/p')
        pmax=$(echo "$sline" | sed -n 's/.*"max_ms": \([0-9.]*\).*/\1/p')
        if [[ -n "$p99" ]]; then
            p99s+=("$p99"); p50s+=("${p50:-nan}")
            echo "  $label ${size}B rep$r: p50=${p50:-?}ms p99=${p99}ms p999=${p999:-?}ms max=${pmax:-?}ms n=${cnt:-?}"
        fi
    done
    # A* trajectory + witness gauges (RWM_DIAG-gated [SPAN] trace on the
    # sending engines). Must be pipeline-failure-safe under set -e +
    # pipefail: a `head` in the pipe SIGPIPEs the upstream, so the line cap
    # lives inside awk and the whole pipeline is `|| true`-guarded.
    for lg in /tmp/tm-s.log /tmp/tm-c.log; do
        { grep -E '^\[SPAN\] ' "$lg" 2>/dev/null || true; } \
            | awk 'NR<=6 || NR%10==0 { n++; if (n<=24) print }' \
            | sed "s|^|  SPAN $label ${size}B ${lg##*/}: |" || true
    done
    # ── The EVICT seat's repair waste (the ρ leg, paper §5.1) ─────
    #
    # This matrix's default arm — `--protocol-hint realtime` without
    # `--window-reliable` — is the ρ < 1 EVICT seat. Per-seq gap ARQ is
    # armed there (`recv_nack_tx` keys on nothing about `reliable`), so the
    # receiver requests repairs for holes it has already licensed itself to
    # discard, and the repair is later than the give-up by construction.
    # `[RFA]` (including `late_after_aban=`, repairs that landed after the
    # hole was abandoned), `[SUCC]`'s abandon count and `[RACK] fa=` measure
    # that waste.
    #
    # Last line only, on both endpoints, `|| true`-guarded like the [SPAN]
    # scrape above: these are cumulative counters (the `[RFA]` convention),
    # the server is SIGKILLed so no `Drop` ever runs, and a missing gauge must
    # be a skipped datum and never a matrix kill. The scrape prints whatever
    # the line carries, so it tolerates absent fields.
    for lg in /tmp/tm-s.log /tmp/tm-c.log; do
        # `[ETA]` and `[LAT]` (`net/eta.rs` / `net/lat.rs`): the placement
        # law's own prediction-error dispersion and the delivered-latency
        # decomposition. Same last-line-wins convention (an exit-flush line
        # carrying `final=1`, when the engine emits one, is the last line and
        # so is the one taken). `eta_s4.py` reads its points off this scrape.
        for tag in '\[RFA\]' '\[SUCC\]' '\[RACK\]' '\[ETA\]' '\[LAT\]'; do
            { grep -E "^${tag} " "$lg" 2>/dev/null || true; } \
                | tail -1 \
                | sed "s|^|  EVICT $label ${size}B ${lg##*/}: |" || true
        done
    done
    hard_cleanup
    if [[ ${#p99s[@]} -gt 0 ]]; then
        printf '%s\n' "${p99s[@]}" | sort -n | awk -v h="$label" -v s="$size" '
            {a[NR]=$1} END{ printf "ARM %s %dB: n=%d min=%.0f median=%.0f max=%.0f\n",
                h,s,NR,a[1],a[int((NR+1)/2)],a[NR] }'
    else
        echo "ARM $label ${size}B: NO_DATA"
    fi
}

if [[ -n "${RWM_TM_ARMS:-}" ]]; then
    echo "=== tail matrix (task #61 flip-gate) @ $CELL seed=$SEED arms='$RWM_TM_ARMS', $REPS reps/arm (warm tunnel), ${TM_RATE}msg/s x${TM_DUR}s sizes='$TM_SIZES' $(date +%T)"
    for arm in $RWM_TM_ARMS; do
        AHINT="realtime"
        case "$arm" in
            # `ship` = env fully unset = the binary's current defaults.
            # `stream` is an alias kept so older arm lists still run.
            ship)    AENV="";              AFLAGS="" ;;
            stream)  AENV="";              AFLAGS="" ;;
            unified) AENV="RWM_UNIFIED=1"; AFLAGS="" ;;
            rlc)     AENV="";              AFLAGS="--fec-backend rlc" ;;
            # The shipped Realtime machine under an explicit default-stack env
            # (STORE_PATHS / RECOV_MP are reliable-window-gated, inert here;
            # the live members at this cell are the anchor pair).
            stack)   AENV="RWM_STORE_PATHS=1 RWM_RECOV_MP=1 RWM_MSTAR_ANCHOR=1 RWM_CLOCK_GAP=1"; AFLAGS="" ;;
            # The substrate-CC tail cell: the shipped machine under BBR
            # (`default`, an alias for env-unset) vs Copa-sole passthrough
            # (ADR-0062).
            default) AENV="";              AFLAGS="" ;;
            copa)    AENV="RWM_QUIC_CC=passthrough"; AFLAGS="" ;;
            # No-regression spots for single gates. Each changes a clock or a
            # path the tail cell is sensitive to, so the spot is a gate, not a
            # formality:
            #   mtu    compact DATA framing
            mtu)     AENV="RWM_WIRE_COMPACT=1"; AFLAGS="" ;;
            #   est    estimator heavy-math cadence, set explicitly. Since
            #          83462ae the cadence is the shipped default, so `est`
            #          runs the same machine as `ship` (kept so older arm
            #          lists still run); the per-ack control is `peracked`.
            est)     AENV="RWM_EST_CADENCE=1"; AFLAGS="" ;;
            #   peracked  the per-ack BOCD update (the pre-83462ae default),
            #          witnessed by the 'cadence OFF' echo on both endpoints
            peracked) AENV="RWM_EST_CADENCE=0"; AFLAGS="" ;;
            #   bbrrs  burst-robust BBR substrate controller
            bbrrs)   AENV="RWM_QUIC_CC=bbr_rs"; AFLAGS="" ;;
            #   uni    removed: the dyn-store-cap phase's path set is the
            #          channel's membership (live_paths()) unconditionally
            #          (plan 2b), so `ship` IS the former arm. Fails loudly
            #          rather than silently re-measuring `ship`.
            uni)
                echo "ARM uni was removed: RWM_STORE_CAP_UNIFIED is gone, the store-cap path set is live_paths() unconditionally. Use 'ship'." >&2
                continue ;;
            #   prior  est cadence and emit batching both explicitly off. The
            #          cadence is ON by default since 83462ae, so `prior` must
            #          carry RWM_EST_CADENCE=0 (env-unset is no longer the
            #          prior machine); the pool anchor no longer follows the
            #          cadence (it resolves off unless RWM_POOL_ANCHOR=1).
            prior)   AENV="RWM_EST_CADENCE=0 RWM_EMIT_BATCH=0"; AFLAGS="" ;;
            #   am     RWM_ACK_MERGE=1 alone (the receiver's control cadence)
            am)      AENV="RWM_ACK_MERGE=1"; AFLAGS="" ;;
            #   tt     the three-term store cap; scoped to the reliable window's
            #          plain dynamic cap, so it must be inert here
            tt)      AENV="RWM_THREE_TERM=1 RWM_PLAIN_RS=1"; AFLAGS="" ;;
            # `streaming`/`bulkstream` named the deleted streaming machine; on
            # current binaries RWM_UNIFIED=0 + Realtime selects the legacy-RLC
            # windowed machine, so these arms fail loudly instead of silently
            # measuring a different machine (use `rlc` / `ship`).
            streaming|bulkstream)
                echo "ARM $arm was removed with the streaming machine (ADR-0064); RWM_UNIFIED=0 now = legacy-RLC. Use 'rlc' or 'ship'." >&2
                continue ;;
            bulkship)   AENV="";              AFLAGS=""; AHINT="bulk" ;;
            *) echo "unknown arm '$arm'" >&2; continue ;;
        esac
        for size in $TM_SIZES; do
            echo "--- $arm ${size}B start=$(date +%T)"
            run_arm "$AHINT" "$size" "$arm" "$AENV" "$AFLAGS"
        done
    done
    echo "=== done $(date +%T)"
    exit 0
fi

echo "=== tail matrix @ $CELL, $REPS reps/arm (warm tunnel), 50msg/s x20s $(date +%T)"
for hint in realtime bulk; do
    for size in 400 1200; do
        echo "--- $hint ${size}B start=$(date +%T)"
        run_arm "$hint" "$size"
    done
done
echo "=== done $(date +%T)"
