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
# meas/streaming-retirement (crown re-test) harness glue: message rate,
# stream duration and size list are overridable so the L2-era stream_bench
# shape (50 msg/s x 30 s, 1200 B) is reproducible through the SAME matrix
# machinery. Defaults = the historic tail_matrix shape, byte-identical.
TM_RATE="${RWM_TM_RATE:-50}"; TM_DUR="${RWM_TM_DUR:-20}"
TM_SIZES="${RWM_TM_SIZES:-400 1200}"
TM_TMO=$((TM_DUR + 10))
# meas/adversarial-cells (ARC B1) harness glue: the topology script is
# overridable (RWM_TM_TOPO=./adv_cells.sh) so the SAME matrix machinery can
# run the realtime-crown row on an adversarial cell (`up <cell> [--seed N]`
# interface shared by topo.sh and adv_cells.sh). Default = topo.sh,
# byte-identical.
TM_TOPO="${RWM_TM_TOPO:-./topo.sh}"

hard_cleanup() {
    pkill -x raptorpath 2>/dev/null || true
    pkill -f 'python3 ./transfer_bench.py' 2>/dev/null || true
    ip netns del "$NS_CLI" 2>/dev/null || true
    ip netns del "$NS_SRV" 2>/dev/null || true
}
trap hard_cleanup EXIT

run_arm() { # hint size label armenv armflags -> one warm tunnel, REPS stream measurements
    local hint="$1" size="$2" label="${3:-$1}" armenv="${4:-}" armflags="${5:-}"
    echo "ARMENV $label ${size}B: hint=$hint env='${armenv:-<unset>}' flags='${armflags:-}' rate=$TM_RATE dur=$TM_DUR"
    # Discipline item 7: lib.sh forces set -e — a transient topo bringup
    # failure must fail THIS arm loudly (the ping probe below catches it),
    # not kill the whole matrix silently (bit the shed battery's c3-s7
    # rlc-1200B arm, 2026-07-21).
    # Crown re-test hardening (the embatch "retry-hardened driver"
    # precedent): per-rep-interleaved invocations cycle netns fast enough
    # to hit transient bringup collisions — retry the WHOLE bringup up to
    # 3 times, each attempt counted loudly (BRINGUP_RETRY), before the arm
    # is declared BRINGUP_FAIL. Captured measurements are never affected:
    # the stream reps only run after a verified ping.
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
    # Mechanism-liveness echoes (MEASUREMENT DISCIPLINE): code-family selection
    # + decoder machine, from BOTH endpoints.  NOTE lib.sh turns on set -e:
    # every pipeline here must be no-match-safe (the rlc arm has no RWM_UNIFIED
    # echo — an unguarded grep kills the whole matrix silently).
    for lg in /tmp/tm-s.log /tmp/tm-c.log; do
        sed 's/\x1b\[[0-9;]*m//g' "$lg" 2>/dev/null \
            | grep -oE '(RWM_UNIFIED[^"]*|Realtime mode: auto-selecting streaming[^"]*|auto-selecting RLC windowed backend|unified span law ACTIVE[^"]*|unified overload shedding ACTIVE[^"]*|A\* send-rate anchor ACTIVE[^"]*|clock-gap estimator hygiene ACTIVE[^"]*|M\* peer-report RTT-feed suppression ACTIVE[^"]*|backend=[A-Za-z]+ sliding-window FEC mode|sliding-window FEC mode[^"]*|quinn congestion controller: BBR[^"]*|RWM_QUIC_CC=passthrough[^"]*|derived patience ACTIVE[^"]*|derived stall gauge ACTIVE[^"]*|estimator heavy-math cadence ACTIVE[^"]*|ack-merge ACTIVE[^"]*)' \
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
        # goal-gate "Unified Shedding": delivered count per rep (the ρ story
        # — shedding must stay within the 1−ρ class; rate*dur msgs sent/rep).
        # Crown re-test glue: p999/max scraped too (the L2-era record's p99.9
        # metric — gated on the 30-s shape, free everywhere else).
        local cnt p999 pmax
        cnt=$(echo "$sline" | sed -n 's/.*"count": \([0-9]*\).*/\1/p')
        p999=$(echo "$sline" | sed -n 's/.*"p999_ms": \([0-9.]*\).*/\1/p')
        pmax=$(echo "$sline" | sed -n 's/.*"max_ms": \([0-9.]*\).*/\1/p')
        if [[ -n "$p99" ]]; then
            p99s+=("$p99"); p50s+=("${p50:-nan}")
            echo "  $label ${size}B rep$r: p50=${p50:-?}ms p99=${p99}ms p999=${p999:-?}ms max=${pmax:-?}ms n=${cnt:-?}"
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
    echo "=== tail matrix (task #61 flip-gate) @ $CELL seed=$SEED arms='$RWM_TM_ARMS', $REPS reps/arm (warm tunnel), ${TM_RATE}msg/s x${TM_DUR}s sizes='$TM_SIZES' $(date +%T)"
    for arm in $RWM_TM_ARMS; do
        AHINT="realtime"
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
            # feat/window-mtu (goal-gate "Window Decoupling + MTU Scaling"):
            # the crown-gate arms — part 2 compact framing (mandatory spot),
            # part 1 decoupled window, and the composed pair.
            mtu)     AENV="RWM_WIRE_COMPACT=1"; AFLAGS="" ;;
            wdfix)   AENV="RWM_WIN_DECOUPLE=1"; AFLAGS="" ;;
            wdmtu)   AENV="RWM_WIN_DECOUPLE=1 RWM_WIRE_COMPACT=1"; AFLAGS="" ;;
            # feat/recv-permsg (goal-gate "Receiver Per-Message Wall"): the
            # crown-gate arm — estimator heavy-math cadence (delivery path
            # touched at the estimator only; the spot is mandatory).
            est)     AENV="RWM_EST_CADENCE=1"; AFLAGS="" ;;
            # fix/shal8-anchor (goal-gate "Ship The Wins 2: shal8 anchor"):
            # the crown-gate arm — burst-robust BBR substrate controller
            # (P-F4; the estimator barely engages app-limited, the spot is
            # mandatory).
            bbrrs)   AENV="RWM_QUIC_CC=bbr_rs"; AFLAGS="" ;;
            # fix/store-cap-triplication (goal-gate "Store-Cap
            # Triplication"): the crown-gate arm - the dyn-store-cap
            # phase's path set moves off the cwnd-saturation-filtered
            # active_paths() onto live_paths(). Realtime is single-path
            # here, which is exactly where the filter EMPTIES the set
            # (component bench: 88.6% of L0 refresh ticks; L1 smoke: 31.3%
            # at c1), so the spot is mandatory.
            uni)     AENV="RWM_STORE_CAP_UNIFIED=1"; AFLAGS="" ;;
            # feat/ship-est-cadence (goal-gate "Ship The Wins 1"): the
            # composed-default crown spot — `ship` (env unset = est+eb+
            # pool-anchor NEW default) vs the prior default (est=0 turns the
            # composed pool-anchor default off with it; eb=0 restores the
            # per-symbol sender).
            prior)   AENV="RWM_EST_CADENCE=0 RWM_EMIT_BATCH=0"; AFLAGS="" ;;
            # feat/pool-delivery-anchor (goal-gate "Ship The Wins 1b"): the
            # attempt-2 crown spot. Defaults were REVERTED at the end of
            # attempt 1, so `ship`/env-unset is now the PRIOR default and the
            # candidates are explicit: `deliv` = est+eb+pool-anchor+the
            # delivery-clocked rate term (arm A), `floorb` = attempt 1's pool
            # + the honest anchor-floor bound (arm B).
            deliv)   AENV="RWM_EST_CADENCE=1 RWM_EMIT_BATCH=1"; AFLAGS="" ;;
            floorb)  AENV="RWM_EST_CADENCE=1 RWM_EMIT_BATCH=1 RWM_POOL_DELIV=0 RWM_FLOOR_BOUND=1"; AFLAGS="" ;;
            # feat/derived-patience (goal-gate "Unlock The Default 2"): the
            # crown spot for THE candidate. `pat` = est+eb+the derived
            # recovery-patience floor; the tail cell is exactly where a
            # patience change could hurt (a floor that fires too eagerly buys
            # throughput with p99), so this arm is a gate, not a formality.
            # Compare against `ship` (env unset = today's default) and
            # `deliv` (est+eb, the same composition WITHOUT the derived floor).
            pat)     AENV="RWM_EST_CADENCE=1 RWM_EMIT_BATCH=1 RWM_PATIENCE_DERIVED=1"; AFLAGS="" ;;
            # feat/ack-merge-flip (goal-gate "Ack-Merge Flip"): the crown
            # NO-REGRESSION spot for the single-knob candidate. `am` =
            # RWM_ACK_MERGE=1 alone against `ship` (env unset = today's
            # default). The knob changes the receiver's control-datagram
            # cadence, which is exactly the clock a tail cell is sensitive
            # to, so this arm is a gate, not a formality.
            am)      AENV="RWM_ACK_MERGE=1"; AFLAGS="" ;;
            # feat/three-term-battery (goal-gate "Three-Term Law"): the crown
            # NO-REGRESSION spot (criterion 5, <= ~41 ms at 1000/1000). `tt` =
            # the SCORED composed arm RWM_THREE_TERM=1 RWM_PLAIN_RS=1 against
            # `ship` (env unset). The law is scoped to the reliable window's
            # plain dynamic cap, so Realtime streaming should be INERT here —
            # which is precisely why the spot is a gate: an inert law that
            # moves the crown has escaped its scope.
            tt)      AENV="RWM_THREE_TERM=1 RWM_PLAIN_RS=1"; AFLAGS="" ;;
            # meas/streaming-retirement (crown re-test) HISTORIC arms: the
            # `streaming`/`bulkstream` arms drove the 2026-07-27 crown re-test
            # (RWM_UNIFIED=0 selected the streaming two-layer machine). The
            # streaming machine was DELETED 2026-07-28 (register RE-TESTED/
            # CLEARED); on current binaries RWM_UNIFIED=0 + Realtime selects
            # the LEGACY-RLC windowed machine, so these arms would silently
            # measure a different machine than their name claims — they now
            # fail loudly instead (use `rlc` / `ship`).
            streaming|bulkstream)
                echo "ARM $arm RETIRED 2026-07-28: streaming machine deleted (goal-gate 'Streaming Crown Re-Test' / register); RWM_UNIFIED=0 now = legacy-RLC. Use 'rlc' or 'ship'." >&2
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
