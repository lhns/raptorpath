#!/bin/bash
# meas/competitive-baseline REALTIME battery: kernel-TCP (NODELAY, cubic) and
# native-QUIC (quinn msg_lat) message streams on the tail_matrix workload —
# 50 msg/s x 20 s, 400/1200 B, one-way delivered latency percentiles, cells
# c2/c3. The raptorpath arm runs separately via
#   RWM_TM_ARMS=ship tail_matrix.sh <cell> <reps>
# (same workload, shipped-default tunnel). tcp and quic arms are interleaved
# per rep here; framing identical everywhere (4-byte length + 8-byte send
# timestamp; shared kernel clock => one-way latency).
# Pre-registration: goal-gate.md "Competitive Baseline (2026-07-21)".
#
#   usage: compet_rt.sh <seed> [reps]
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
SEED_ARG="$1"; REPS="${2:-8}"
MSGLAT=/home/vibe/quinn/target/release/examples/msg_lat
TB=/home/vibe/raptorpath/raptorpath/tools/l1/transfer_bench.py
OUT=/home/vibe/compet/rt-s${SEED_ARG}.log
mkdir -p /home/vibe/compet
: > "$OUT"
log() { echo "$@" >> "$OUT"; }

log "# competitive-baseline REALTIME $(date -u +%FT%TZ) seed=$SEED_ARG reps=$REPS 50msg/s x20s"
log "# kernel: $(uname -r)  msg_lat: $(sha256sum $MSGLAT 2>/dev/null | cut -c1-16)"
lscpu | grep "Model name" >> "$OUT"
log ""

teardown() {
    sudo pkill -x msg_lat 2>/dev/null || true
    sudo pkill -f 'transfer_bench.py stream' 2>/dev/null || true
    sudo bash topo.sh down >/dev/null 2>&1 || true
}

run_tcp_rep() { # cell size rep
    local cell="$1" size="$2" rep="$3"
    sudo pkill -f 'transfer_bench.py stream' 2>/dev/null || true
    # NOTE: the log must be (re)creatable by THIS shell's redirect — a
    # root-owned leftover (the first s42 pass used `sudo tee`) makes the
    # redirect fail silently and the server never starts (32/32 NO_SUMMARY).
    sudo rm -f /tmp/compet-tcp-srv.log
    sudo ip netns exec rp-srv timeout 40 python3 "$TB" stream-server \
        --bind 10.77.0.2 --port 9910 >/tmp/compet-tcp-srv.log 2>&1 &
    local spid=$!
    sleep 0.5
    sudo timeout 40 ip netns exec rp-cli python3 "$TB" stream-client \
        --host 10.77.0.2 --port 9910 --rate 50 --duration 20 --size "$size" \
        --cc cubic >/dev/null 2>&1 || true
    wait $spid 2>/dev/null || true
    local s
    s=$({ grep '"summary"' /tmp/compet-tcp-srv.log || true; } | tail -1)
    log "TCP $cell ${size}B rep$rep: ${s:-NO_SUMMARY}"
}

run_quic_rep() { # cell size rep
    local cell="$1" size="$2" rep="$3"
    sudo pkill -x msg_lat 2>/dev/null || true
    sudo ip netns exec rp-srv "$MSGLAT" server --listen 10.77.0.2:9920 \
        >/tmp/compet-quic-srv.log 2>/tmp/compet-quic-srv.err &
    sleep 0.8
    sudo timeout 150 ip netns exec rp-cli "$MSGLAT" client \
        --server-name raptorpath --ip 10.77.0.2:9920 --rate 50 \
        --size "$size" --duration 20 >/tmp/compet-quic-cli.log 2>/dev/null || true
    sleep 1
    local s
    s=$({ grep '"summary"' /tmp/compet-quic-srv.log || true; } | tail -1)
    log "QUIC $cell ${size}B rep$rep: ${s:-NO_SUMMARY ($(tail -1 /tmp/compet-quic-srv.err 2>/dev/null))}"
    sudo pkill -x msg_lat 2>/dev/null || true
}

for CELL in c2 c3; do
    teardown
    sudo bash topo.sh up "$CELL" --seed "$SEED_ARG" >/dev/null 2>&1 || { log "TOPO-FAIL $CELL"; continue; }
    log "--- cell $CELL up (seed $SEED_ARG) $(date -u +%T)"
    for SIZE in 400 1200; do
        for REP in $(seq 1 $REPS); do
            run_tcp_rep  "$CELL" "$SIZE" "$REP"
            run_quic_rep "$CELL" "$SIZE" "$REP"
        done
    done
done
teardown

log "--- ARMCOUNTS (expect $REPS lines per arm-cell-size)"
for CELL in c2 c3; do for SIZE in 400 1200; do
    t=$(grep -c "^TCP $CELL ${SIZE}B" "$OUT" || true)
    q=$(grep -c "^QUIC $CELL ${SIZE}B" "$OUT" || true)
    log "ARMCOUNT tcp-$CELL-${SIZE}B n=$t quic-$CELL-${SIZE}B n=$q"
done; done
log "RT-DONE seed=$SEED_ARG $(date -u +%FT%TZ)"
echo RT-DONE-$SEED_ARG
