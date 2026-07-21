#!/bin/bash
# meas/competitive-baseline BULK battery: the SHIPPED-DEFAULT raptorpath stack
# vs native QUIC (quinn-perf), kernel TCP (iperf3 cubic/bbr) and kernel MPTCP
# (IPPROTO_MPTCP via transfer_bench.py) on the standard cells — 25 MB objects,
# goodput, arms interleaved round-robin per rep, fresh topology per invocation.
# Pre-registration: goal-gate.md "Competitive Baseline (2026-07-21)".
#
# Cells:
#   singles c1/c2/c3: rp (perf_rwm_c single), quinn (UPLOAD direction — the
#     GE direction, unlike Phase 2's download), tcp-cubic, tcp-bbr
#   duals c7 (c2+c2) / c8 (c2+c3): rp-dual, tcp-bbr-pathA (best single-path
#     competitor, same session), mptcp-cubic, mptcp-bbr (per-netns sysctl CC)
#
# Direction discipline: EVERY sender sits in rp-cli => the object always
# traverses the lossy (cli->srv) netem direction.
#
#   usage: compet_battery.sh <seed> [reps]
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
SEED_ARG="$1"; REPS="${2:-8}"
BIN=/home/vibe/raptorpath/target/release/raptorpath
QPERF=/home/vibe/quinn/target/release/quinn-perf
TB=/home/vibe/raptorpath/raptorpath/tools/l1/transfer_bench.py
OUT=/home/vibe/compet/bulk-s${SEED_ARG}.log
DDIR=/home/vibe/compet/diag
mkdir -p "$DDIR" /home/vibe/compet
: > "$OUT"
BYTES=25000000

log() { echo "$@" >> "$OUT"; }

log "# competitive-baseline BULK $(date -u +%FT%TZ) seed=$SEED_ARG reps=$REPS bytes=$BYTES"
log "# binary: $(sha256sum $BIN)"
log "# source: $(cat /home/vibe/raptorpath/COMMIT 2>/dev/null || echo unknown)"
log "# kernel: $(uname -r)"
log "# iperf3: $(iperf3 --version 2>/dev/null | head -1)"
log "# quinn: $(git -C /home/vibe/quinn rev-parse HEAD 2>/dev/null || echo unknown)"
log "# tcp_cc_avail: $(sysctl -n net.ipv4.tcp_available_congestion_control 2>/dev/null)"
log "# mptcp.enabled(host): $(sysctl -n net.mptcp.enabled 2>/dev/null || echo ABSENT)"
lscpu | grep "Model name" >> "$OUT"
(lscpu | grep -E "^Flags" | grep -oE "aes|avx2|pclmulqdq" | sort -u | tr '\n' ' ' >> "$OUT") || true
log ""

teardown() {
    sudo pkill -x raptorpath 2>/dev/null || true
    sudo pkill -f quinn-perf 2>/dev/null || true
    sudo pkill -f 'iperf3 -s' 2>/dev/null || true
    sudo pkill -f 'transfer_bench.py server' 2>/dev/null || true
    sudo bash topo_dual.sh down >/dev/null 2>&1 || true
    sudo bash topo.sh down >/dev/null 2>&1 || true
}

# ---- rp shipped default (plain window reliable; liveness echoes asserted) --
run_rp() { # name cellA cellB mode
    local name="$1" ca="$2" cb="$3" mode="$4"
    log "=== rep=$REP arm=$name seed=$SEED_ARG cell=$ca/$cb/$mode $(date -u +%T)"
    sudo env SEED=$SEED_ARG RWM_GEN=0 RWM_DIAG=1 bash perf_rwm_c.sh $ca $cb bulk $BYTES 1 $mode 2>&1 \
        | grep -E "summary|\"dnf\"|CPU:" >> "$OUT" || true
    local sr mp ms gap
    sr=$(grep -c "SACK-clocked store release ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
    mp=$(grep -c "multipath recovery suppression ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
    ms=$(grep -c "peer-report RTT-feed suppression ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
    gap=$(grep -c "clock-gap estimator hygiene ACTIVE" /tmp/rwm-c.log 2>/dev/null || true)
    log "LIVENESS sr=$sr mp=$mp ms=$ms gap=$gap (shipped defaults; all expected >0)"
    if [ "${sr:-0}" -eq 0 ]; then log "ARM-LIVENESS-FAIL-SR $name rep=$REP"; fi
    (sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log | grep '\[DIAG\]' | tail -1 \
        | grep -oE "win=[0-9]+/[0-9]+|retx=[0-9]+|pl=[0-9.]+" | tr '\n' ' ' >> "$OUT") || true
    log ""
    cp /tmp/rwm-c.log "$DDIR/${name}-s${SEED_ARG}-r${REP}-c.log" 2>/dev/null || true
    cp /tmp/rwm-s.log "$DDIR/${name}-s${SEED_ARG}-r${REP}-s.log" 2>/dev/null || true
}

# ---- quinn-perf, UPLOAD (= lossy) direction; CC via --congestion -----------
# quinn-perf exposes --congestion {cubic,bbr,new-reno}; stock default = cubic
# (verified via --help on the VM). The CLIENT is the upload sender, so the
# client's CC governs the data direction.
run_quinn() { # name cell duration cc
    local name="$1" cell="$2" dur="$3" cc="$4"
    log "=== rep=$REP arm=$name seed=$SEED_ARG cell=$cell dur=$dur cc=$cc $(date -u +%T)"
    teardown
    sudo bash topo.sh up "$cell" --seed "$SEED_ARG" >/dev/null 2>&1 || { log "TOPO-FAIL $name"; return; }
    sudo ip netns exec rp-srv "$QPERF" server --listen 10.77.0.2:4433 \
        >"$DDIR/${name}-s${SEED_ARG}-r${REP}-srv.log" 2>&1 &
    sleep 0.7
    local out
    out=$(sudo timeout $((dur + 90)) ip netns exec rp-cli /usr/bin/time -v \
        "$QPERF" client raptorpath:4433 --ip 10.77.0.2 \
        --upload-size ${BYTES} --download-size 0 --congestion "$cc" \
        --duration "$dur" --interval "$dur" --json - \
        2>"$DDIR/${name}-s${SEED_ARG}-r${REP}-time.log" | tail -1) || true
    log "QUINN-JSON $out"
    (grep -E "User time|System time" "$DDIR/${name}-s${SEED_ARG}-r${REP}-time.log" \
        | tr '\n' ' ' >> "$OUT") || true
    log ""
    sudo pkill -f quinn-perf 2>/dev/null || true
}

# ---- kernel TCP via iperf3 (server rp-srv, client rp-cli = lossy dir) ------
run_iperf() { # name cell cc topo(single|dualA) [cellB]
    local name="$1" cell="$2" cc="$3" topo="$4" cellb="${5:-}"
    log "=== rep=$REP arm=$name seed=$SEED_ARG cell=$cell cc=$cc topo=$topo $(date -u +%T)"
    teardown
    if [ "$topo" = "dualA" ]; then
        sudo bash topo_dual.sh up "$cell" "$cellb" --seed "$SEED_ARG" >/dev/null 2>&1 || { log "TOPO-FAIL $name"; return; }
    else
        sudo bash topo.sh up "$cell" --seed "$SEED_ARG" >/dev/null 2>&1 || { log "TOPO-FAIL $name"; return; }
    fi
    sudo ip netns exec rp-srv iperf3 -s -D --pidfile /tmp/rp-iperf3.pid 2>/dev/null || true
    sleep 0.4
    local out
    out=$(sudo timeout 400 ip netns exec rp-cli iperf3 -c 10.77.0.2 -n $BYTES -C "$cc" --json 2>/dev/null) || true
    if [ -n "$out" ]; then
        echo "$out" | jq -c "{arm:\"$name\",rep:$REP,seconds:.end.sum_sent.seconds,mbps:(.end.sum_sent.bits_per_second/1e6),retransmits:.end.sum_sent.retransmits,cc:.end.sender_tcp_congestion}" >> "$OUT" \
            || log "IPERF-PARSE-FAIL $name rep=$REP"
    else
        log "{\"arm\":\"$name\",\"rep\":$REP,\"dnf\":true,\"timeout_s\":400}"
    fi
    sudo pkill -f 'iperf3 -s' 2>/dev/null || true
}

# ---- kernel MPTCP via iperf3 --mptcp (3.19.1; CC via -C on the MPTCP sock,
# plus per-netns sysctl default as belt-and-braces). Liveness = MPTcpExt
# MPJoin counters in the client netns (subflow actually joined).
run_mptcp() { # name cellA cellB cc
    local name="$1" ca="$2" cb="$3" cc="$4"
    log "=== rep=$REP arm=$name seed=$SEED_ARG cell=$ca+$cb cc=$cc $(date -u +%T)"
    teardown
    sudo bash topo_dual.sh up "$ca" "$cb" --seed "$SEED_ARG" >/dev/null 2>&1 || { log "TOPO-FAIL $name"; return; }
    for ns in rp-cli rp-srv; do
        sudo ip netns exec $ns sysctl -qw net.ipv4.tcp_congestion_control="$cc" || log "SYSCTL-CC-FAIL $ns $cc"
    done
    local pre post
    pre=$(sudo ip netns exec rp-cli grep -A1 MPTcpExt /proc/net/netstat 2>/dev/null | tail -1)
    sudo ip netns exec rp-srv iperf3 -s -D -m --pidfile /tmp/rp-iperf3.pid 2>/dev/null || true
    sleep 0.4
    local out
    out=$(sudo timeout 400 ip netns exec rp-cli iperf3 -c 10.77.0.2 -n $BYTES -m -C "$cc" --json 2>/dev/null) || true
    if [ -n "$out" ]; then
        echo "$out" | jq -c "{arm:\"$name\",rep:$REP,seconds:.end.sum_sent.seconds,mbps:(.end.sum_sent.bits_per_second/1e6),retransmits:.end.sum_sent.retransmits}" >> "$OUT" \
            || log "MPTCP-PARSE-FAIL $name rep=$REP"
    else
        log "{\"arm\":\"$name\",\"rep\":$REP,\"dnf\":true,\"timeout_s\":400}"
    fi
    post=$(sudo ip netns exec rp-cli grep -A1 MPTcpExt /proc/net/netstat 2>/dev/null | tail -1)
    log "MPTCP-MIB-HDR $(sudo ip netns exec rp-cli grep -A1 MPTcpExt /proc/net/netstat 2>/dev/null | head -1)"
    log "MPTCP-MIB-PRE $pre"
    log "MPTCP-MIB-POST $post"
    sudo pkill -f 'iperf3 -s' 2>/dev/null || true
}

for REP in $(seq 1 $REPS); do
    # ---- singles ----
    for CELL in c1 c2 c3; do
        run_rp    rp-$CELL      $CELL $CELL single
        if [ "$CELL" = "c3" ]; then QD=60; else QD=30; fi
        run_quinn quinn-cubic-$CELL $CELL $QD cubic
        run_quinn quinn-bbr-$CELL   $CELL $QD bbr
        run_iperf tcp-cubic-$CELL $CELL cubic single
        run_iperf tcp-bbr-$CELL   $CELL bbr   single
    done
    # ---- duals ----
    run_rp    rp-c7          c2 c2 dual
    run_iperf tcp-bbr-c7A    c2 bbr dualA c2
    run_mptcp mptcp-cubic-c7 c2 c2 cubic
    run_mptcp mptcp-bbr-c7   c2 c2 bbr
    run_rp    rp-c8          c2 c3 dual
    run_iperf tcp-bbr-c8A    c2 bbr dualA c3
    run_mptcp mptcp-cubic-c8 c2 c3 cubic
    run_mptcp mptcp-bbr-c8   c2 c3 bbr
done
teardown

log "--- ARMCOUNTS (expect $REPS headers per arm)"
for a in rp-c1 quinn-cubic-c1 quinn-bbr-c1 tcp-cubic-c1 tcp-bbr-c1 \
         rp-c2 quinn-cubic-c2 quinn-bbr-c2 tcp-cubic-c2 tcp-bbr-c2 \
         rp-c3 quinn-cubic-c3 quinn-bbr-c3 tcp-cubic-c3 tcp-bbr-c3 \
         rp-c7 tcp-bbr-c7A mptcp-cubic-c7 mptcp-bbr-c7 \
         rp-c8 tcp-bbr-c8A mptcp-cubic-c8 mptcp-bbr-c8; do
    hdr=$(grep -c "arm=$a " "$OUT" || true)
    log "ARMCOUNT $a headers=$hdr"
    if [ "${hdr:-0}" -eq 0 ]; then log "ARM-VANISHED $a"; fi
done
log "BATTERY-DONE seed=$SEED_ARG $(date -u +%FT%TZ)"
echo BULK-DONE-$SEED_ARG
