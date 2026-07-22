#!/bin/bash
# meas/competitive-baseline seed-7 top-up: the seed-7 topo-ping double-abort
# class (MEASUREMENT DISCIPLINE item 8) thinned several arms (rp-c7/c8 n=2,
# mptcp-bbr-c8 n=2, rp-c2 n=4 ...). Interleaved top-up reps, appending to
# bulk-s<seed>-topup.log; same binary, same protocol; a failed topo bringup
# is retried once per invocation (the documented abort-retry protocol), and
# still-failed invocations are logged.
#   usage: compet_topup.sh <seed> [reps]
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
SEED_ARG="$1"; REPS="${2:-6}"
BIN=/home/vibe/raptorpath/target/release/raptorpath
TB=/home/vibe/raptorpath/raptorpath/tools/l1/transfer_bench.py
OUT=/home/vibe/compet/bulk-s${SEED_ARG}-topup.log
DDIR=/home/vibe/compet/diag
mkdir -p "$DDIR" /home/vibe/compet
: > "$OUT"
BYTES=25000000
log() { echo "$@" >> "$OUT"; }
log "# competitive-baseline TOPUP $(date -u +%FT%TZ) seed=$SEED_ARG reps=$REPS bytes=$BYTES"
log "# binary: $(sha256sum $BIN)"

teardown() {
    sudo pkill -x raptorpath 2>/dev/null || true
    sudo pkill -f 'iperf3 -s' 2>/dev/null || true
    sudo pkill -f 'transfer_bench.py server' 2>/dev/null || true
    sudo bash topo_dual.sh down >/dev/null 2>&1 || true
    sudo bash topo.sh down >/dev/null 2>&1 || true
}

topo_dual_retry() { # cellA cellB
    sudo bash topo_dual.sh up "$1" "$2" --seed "$SEED_ARG" >/dev/null 2>&1 && return 0
    log "TOPO-RETRY $1+$2"
    sudo bash topo_dual.sh up "$1" "$2" --seed "$SEED_ARG" >/dev/null 2>&1
}

run_rp() { # name cellA cellB mode  (perf_rwm_c does its own bringup; retry once on dnf-less no-summary)
    local name="$1" ca="$2" cb="$3" mode="$4" try out
    for try in 1 2; do
        log "=== rep=$REP arm=$name seed=$SEED_ARG cell=$ca/$cb/$mode try=$try $(date -u +%T)"
        out=$(sudo env SEED=$SEED_ARG RWM_GEN=0 RWM_DIAG=1 bash perf_rwm_c.sh $ca $cb bulk $BYTES 1 $mode 2>&1 \
            | grep -E "summary|\"dnf\"|CPU:") || true
        echo "$out" >> "$OUT"
        local sr
        sr=$(grep -c "SACK-clocked store release ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
        log "LIVENESS sr=$sr"
        echo "$out" | grep -q "summary" && return 0
        log "RP-NO-SUMMARY $name rep=$REP try=$try (topo-abort class; retrying once)"
    done
}

run_tb() { # name cellA cellB cc proto bindmode(dual|dualA)
    local name="$1" ca="$2" cb="$3" cc="$4" proto="$5"
    log "=== rep=$REP arm=$name seed=$SEED_ARG cell=$ca+$cb cc=$cc proto=$proto $(date -u +%T)"
    teardown
    topo_dual_retry "$ca" "$cb" || { log "TOPO-FAIL $name"; return; }
    for ns in rp-cli rp-srv; do
        sudo ip netns exec $ns sysctl -qw net.ipv4.tcp_congestion_control="$cc" || true
    done
    sudo ip netns exec rp-srv python3 "$TB" server --bind 10.77.0.2 --port 9900 --proto "$proto" \
        >"$DDIR/${name}-s${SEED_ARG}-topup-r${REP}-srv.log" 2>&1 &
    sleep 1.0
    local ccarg=""
    [ "$proto" = "tcp" ] && ccarg="--cc $cc"
    sudo timeout 400 ip netns exec rp-cli python3 "$TB" client --host 10.77.0.2 --port 9900 \
        --proto "$proto" $ccarg --bytes $BYTES --runs 1 2>/dev/null \
        | sed "s/^/TB-$name /" >> "$OUT" || log "{\"arm\":\"$name\",\"rep\":$REP,\"dnf\":true}"
    if [ "$proto" = "mptcp" ]; then
        log "MPTCP-MIB-POST $(sudo ip netns exec rp-cli grep -A1 MPTcpExt /proc/net/netstat 2>/dev/null | tail -1)"
    fi
    sudo pkill -f 'transfer_bench.py server' 2>/dev/null || true
}

run_iperf_mptcp() { # name cellA cellB cc
    local name="$1" ca="$2" cb="$3" cc="$4"
    log "=== rep=$REP arm=$name seed=$SEED_ARG cell=$ca+$cb cc=$cc $(date -u +%T)"
    teardown
    topo_dual_retry "$ca" "$cb" || { log "TOPO-FAIL $name"; return; }
    for ns in rp-cli rp-srv; do
        sudo ip netns exec $ns sysctl -qw net.ipv4.tcp_congestion_control="$cc" || true
    done
    sudo ip netns exec rp-srv iperf3 -s -D -m --pidfile /tmp/rp-iperf3.pid 2>/dev/null || true
    sleep 0.5
    local out
    out=$(sudo timeout 400 ip netns exec rp-cli iperf3 -c 10.77.0.2 -n $BYTES -m -C "$cc" --json 2>/dev/null) || true
    if [ -n "$out" ]; then
        echo "$out" | jq -c "{arm:\"$name\",rep:$REP,seconds:.end.sum_sent.seconds,mbps:(.end.sum_sent.bits_per_second/1e6),retransmits:.end.sum_sent.retransmits}" >> "$OUT" || log "PARSE-FAIL $name"
    else
        log "{\"arm\":\"$name\",\"rep\":$REP,\"dnf\":true}"
    fi
    sudo pkill -f 'iperf3 -s' 2>/dev/null || true
}

for REP in $(seq 1 $REPS); do
    run_rp rp-c2 c2 c2 single
    run_rp rp-c3 c3 c3 single
    run_rp rp-c7 c2 c2 dual
    run_tb tbmptcp-bbr-c7 c2 c2 bbr mptcp
    run_iperf_mptcp mptcp-bbr-c7 c2 c2 bbr
    run_tb tbtcp-bbr-c7A c2 c2 bbr tcp
    run_rp rp-c8 c2 c3 dual
    run_tb tbmptcp-bbr-c8 c2 c3 bbr mptcp
    run_iperf_mptcp mptcp-bbr-c8 c2 c3 bbr
    run_tb tbtcp-bbr-c8A c2 c3 bbr tcp
done
teardown
log "TOPUP-DONE seed=$SEED_ARG $(date -u +%FT%TZ)"
echo TOPUP-DONE-$SEED_ARG
