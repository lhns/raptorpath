#!/bin/bash
# Realtime-vs-bulk tail comparison, robust + fast: bring the tunnel up ONCE
# per arm, run N stream measurements through the SAME warm tunnel (no flaky
# per-rep bringup), report the p99 DISTRIBUTION (single-run p99 is
# variance-dominated). Matrix: {realtime,bulk} x {400,1200}B at <cell>.
# Every arm is hard-timeout-bounded so nothing can wedge the matrix.
#   sudo bash tail_matrix.sh <cell> <reps>
#
# Task #61 (paper 16.20) 3-arm flip-gate mode: RWM_TM_ARMS="stream unified rlc"
# runs the REALTIME hint only, once per named code-family arm x size:
#   stream  = shipped default (Realtime auto-selects the streaming two-layer)
#   unified = RWM_UNIFIED=1 (Realtime rides the RLC family on the unified
#             global decoder + span law)
#   rlc     = --fec-backend rlc (legacy RlcWindowDecoder realtime)
# Mechanism-liveness echoes (backend selection / "unified global decoder")
# are scraped from both endpoint logs per arm.  SEED env forwards to topo.sh
# (default 42).  Unset RWM_TM_ARMS = legacy behavior, byte-identical.
set -uo pipefail
trap 'echo "EXIT rc=$? ($(date +%T))"' EXIT
cd "$(dirname "$0")"
source ./lib.sh
BIN="/home/vibe/raptorpath/target/release/raptorpath"
CELL="${1:-c2}"; REPS="${2:-5}"
SEED="${SEED:-42}"

hard_cleanup() {
    pkill -x raptorpath 2>/dev/null || true
    pkill -f 'python3 ./transfer_bench.py' 2>/dev/null || true
    ip netns del "$NS_CLI" 2>/dev/null || true
    ip netns del "$NS_SRV" 2>/dev/null || true
}
trap hard_cleanup EXIT

run_arm() { # hint size label armenv armflags -> one warm tunnel, REPS stream measurements
    local hint="$1" size="$2" label="${3:-$1}" armenv="${4:-}" armflags="${5:-}"
    hard_cleanup; sleep 0.5
    # Discipline item 7: lib.sh forces set -e — a transient topo bringup
    # failure must fail THIS arm loudly (the ping probe below catches it),
    # not kill the whole matrix silently (bit the shed battery's c3-s7
    # rlc-1200B arm, 2026-07-21).
    bash ./topo.sh up "$CELL" --seed "$SEED" >/dev/null 2>&1 || true
    # shellcheck disable=SC2086
    ip netns exec "$NS_SRV" env $armenv "$BIN" run --server --bind 10.77.0.2:7000 \
        --tun-name rpsrv0 --tun-addr 10.99.0.2/24 --protocol-hint "$hint" $armflags \
        >/tmp/tm-s.log 2>&1 &
    sleep 2
    # shellcheck disable=SC2086
    ip netns exec "$NS_CLI" env $armenv "$BIN" run --peer 10.77.0.2:7000 --bind 10.77.0.1:0 \
        --tun-name rpcli0 --tun-addr 10.99.0.1/24 --protocol-hint "$hint" $armflags \
        >/tmp/tm-c.log 2>&1 &
    local up=0
    for i in $(seq 1 20); do
        ip netns exec "$NS_CLI" ping -c1 -W1 10.99.0.2 >/dev/null 2>&1 && { up=1; break; }
        sleep 1
    done
    [[ $up -eq 0 ]] && { echo "ARM $label ${size}B: BRINGUP_FAIL"; hard_cleanup; return; }
    # Mechanism-liveness echoes (MEASUREMENT DISCIPLINE): code-family selection
    # + decoder machine, from BOTH endpoints.  NOTE lib.sh turns on set -e:
    # every pipeline here must be no-match-safe (the rlc arm has no RWM_UNIFIED
    # echo — an unguarded grep kills the whole matrix silently).
    for lg in /tmp/tm-s.log /tmp/tm-c.log; do
        sed 's/\x1b\[[0-9;]*m//g' "$lg" 2>/dev/null \
            | grep -oE '(RWM_UNIFIED[^"]*|Realtime mode: auto-selecting streaming[^"]*|auto-selecting RLC windowed backend|unified span law ACTIVE[^"]*|unified overload shedding ACTIVE[^"]*|A\* send-rate anchor ACTIVE[^"]*|clock-gap estimator hygiene ACTIVE[^"]*|M\* peer-report RTT-feed suppression ACTIVE[^"]*|backend=[A-Za-z]+ sliding-window FEC mode|sliding-window FEC mode[^"]*|quinn congestion controller: BBR[^"]*|RWM_QUIC_CC=passthrough[^"]*)' \
            | sort -u | sed "s|^|  ECHO $label ${size}B ${lg##*/}: |" || true
    done
    local p99s=() p50s=()
    for r in $(seq 1 "$REPS"); do
        : > /tmp/tm-srv.log
        ip netns exec "$NS_SRV" timeout 30 python3 ./transfer_bench.py stream-server \
            --bind 10.99.0.2 --port 9910 >/tmp/tm-srv.log 2>&1 &
        local spid=$!
        sleep 0.5
        timeout 30 ip netns exec "$NS_CLI" python3 ./transfer_bench.py stream-client \
            --host 10.99.0.2 --port 9910 --rate 50 --duration 20 --size "$size" \
            >/dev/null 2>&1 || true
        wait $spid 2>/dev/null || true
        local p99 p50
        # no-summary-safe under lib.sh's set -e (a timed-out rep must be a
        # skipped datum, not a matrix kill)
        p99=$({ grep '"summary"' /tmp/tm-srv.log || true; } | tail -1 \
              | sed -n 's/.*"p99_ms": \([0-9.]*\).*/\1/p')
        p50=$({ grep '"summary"' /tmp/tm-srv.log || true; } | tail -1 \
              | sed -n 's/.*"p50_ms": \([0-9.]*\).*/\1/p')
        # goal-gate "Unified Shedding": delivered count per rep (the ρ story
        # — shedding must stay within the 1−ρ class; 1000 msgs sent/rep).
        local cnt
        cnt=$({ grep '"summary"' /tmp/tm-srv.log || true; } | tail -1 \
              | sed -n 's/.*"count": \([0-9]*\).*/\1/p')
        if [[ -n "$p99" ]]; then
            p99s+=("$p99"); p50s+=("${p50:-nan}")
            echo "  $label ${size}B rep$r: p50=${p50:-?}ms p99=${p99}ms n=${cnt:-?}"
        fi
    done
    # feat/anchor-hygiene: A* trajectory + witness gauges (RWM_DIAG-gated
    # [SPAN] trace on the sending engines). MUST be pipeline-failure-safe
    # under lib.sh's set -e + pipefail: a `head` in the pipe SIGPIPEs the
    # upstream and killed the s42 pass after one arm (MEASUREMENT
    # DISCIPLINE item 7 recurrence, recorded in "Anchor Hygiene") — the
    # line cap lives inside awk and the whole pipeline is `|| true`-guarded.
    for lg in /tmp/tm-s.log /tmp/tm-c.log; do
        { grep -E '^\[SPAN\] ' "$lg" 2>/dev/null || true; } \
            | awk 'NR<=6 || NR%10==0 { n++; if (n<=24) print }' \
            | sed "s|^|  SPAN $label ${size}B ${lg##*/}: |" || true
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
    echo "=== tail matrix (task #61 flip-gate) @ $CELL seed=$SEED arms='$RWM_TM_ARMS', $REPS reps/arm (warm tunnel), 50msg/s x20s $(date +%T)"
    for arm in $RWM_TM_ARMS; do
        case "$arm" in
            # meas/competitive-baseline: `ship` = env fully unset = whatever the
            # binary's CURRENT defaults are (post-unified-flip: the unified
            # machine + the consolidation stack). `stream` predates the flip
            # (env-empty then meant the streaming machine); kept for battery
            # reproducibility — on a post-flip binary the two are identical.
            ship)    AENV="";              AFLAGS="" ;;
            stream)  AENV="";              AFLAGS="" ;;
            unified) AENV="RWM_UNIFIED=1"; AFLAGS="" ;;
            rlc)     AENV="";              AFLAGS="--fec-backend rlc" ;;
            # feat/consolidation: the shipped streaming Realtime machine UNDER
            # the candidate default stack env (the tail-crown regression gate
            # — the 12-48x property must survive the stack). STORE_PATHS /
            # RECOV_MP are reliable-window-gated (inert here by construction);
            # the live members at this cell are the anchor pair.
            stack)   AENV="RWM_STORE_PATHS=1 RWM_RECOV_MP=1 RWM_MSTAR_ANCHOR=1 RWM_CLOCK_GAP=1"; AFLAGS="" ;;
            # feat/copa-sole-clean: the substrate-CC tail cell — the shipped
            # default machine (unified Realtime, post-flip) under BBR-under
            # (default) vs Copa-sole passthrough. `default` is an alias for
            # env-unset (the historical `stream` name predates the unified
            # default flip).
            default) AENV="";              AFLAGS="" ;;
            copa)    AENV="RWM_QUIC_CC=passthrough"; AFLAGS="" ;;
            *) echo "unknown arm '$arm'" >&2; continue ;;
        esac
        for size in 400 1200; do
            echo "--- $arm ${size}B start=$(date +%T)"
            run_arm realtime "$size" "$arm" "$AENV" "$AFLAGS"
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
