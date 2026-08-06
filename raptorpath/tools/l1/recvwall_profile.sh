#!/bin/bash
# feat/recv-permsg STEP 1 — profile the RECEIVER core-second at the c1 wall
# (goal-gate "Receiver Per-Message Wall" (d)). FOREGROUND polling only;
# rp-* netns only.
#   usage: recvwall_profile.sh <phase> [envs]
#     rp-perf-srv    rp c1 single 1.2GB: perf record -F 397 -g on the SERVER (receiver)
#     rp-strace-srv  rp c1 single 1.2GB: strace -c 10 s on the SERVER, then client; snmp counters
#     quinn-recv     quinn-perf bbr upload on c1: server (receiver) CPU + strace -c
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
BIN=/home/vibe/raptorpath/target/release/raptorpath
QPERF=/home/vibe/quinn/target/release/quinn-perf
OUT=/home/vibe/recvwall/profile-$1.log
EXTRA_ENV="${2:-}"
mkdir -p /home/vibe/recvwall
: > "$OUT"
log() { echo "$@" >> "$OUT"; }
log "# recvwall profile phase=$1 extra_env='$EXTRA_ENV' $(date -u +%FT%TZ)"
log "# binary: $(sha256sum $BIN 2>/dev/null)"
log "# source: $(cat /home/vibe/raptorpath/COMMIT 2>/dev/null || echo unknown)"
log "# kernel: $(uname -r)"
lscpu | grep "Model name" >> "$OUT"
log ""

counters() { # ns
    sudo ip netns exec "$1" sh -c '
        grep -E "cli0|srv0" /proc/net/dev
        awk "/^Udp: [0-9]/ {print \"UDPSNMP out=\" \$5, \"in=\" \$2}" /proc/net/snmp'
}

teardown() {
    sudo pkill -x raptorpath 2>/dev/null || true
    sudo pkill -f "quinn.*perf" 2>/dev/null || true
    sudo bash topo_dual.sh down >/dev/null 2>&1 || true
    sudo bash topo.sh down >/dev/null 2>&1 || true
}

find_pid() { # role(--server|--client)
    for q in $(pgrep -x raptorpath); do
        grep -qa -- "$1" /proc/$q/cmdline && { echo $q; return; }
    done
}

case "$1" in
rp-perf-srv|rp-strace-srv)
    teardown
    BYTES=1200000000
    sudo env SEED=42 RWM_GEN=0 RWM_DIAG=1 $EXTRA_ENV bash perf_rwm_c.sh c1 c1 bulk $BYTES 1 single \
        > /home/vibe/recvwall/profrun-$1.out 2>&1 &
    RUNNER=$!
    SRVPID=""; CLIPID=""
    for _ in $(seq 1 60); do
        CLIPID=$(find_pid -- --client); SRVPID=$(find_pid -- --server)
        [ -n "$CLIPID" ] && [ -n "$SRVPID" ] && break
        sleep 0.5
    done
    if [ -z "$SRVPID" ]; then log "FATAL no server pid"; teardown; exit 1; fi
    log "server pid=$SRVPID client pid=$CLIPID bytes=$BYTES"
    sleep 6
    if [ "$1" = "rp-strace-srv" ]; then
        log "== SNMP/dev BEFORE (srv ns)"; counters rp-srv >> "$OUT" 2>&1
        T0=$(date +%s%N)
        sudo timeout -s INT 10 strace -c -f -p "$SRVPID" -o /home/vibe/recvwall/strace-srv.txt >/dev/null 2>>/home/vibe/recvwall/strace-err.txt || true
        T1=$(date +%s%N)
        log "== SNMP/dev AFTER (srv ns, strace window $(( (T1-T0)/1000000 )) ms)"; counters rp-srv >> "$OUT" 2>&1
        log "== strace -c SERVER (receiver)"; cat /home/vibe/recvwall/strace-srv.txt >> "$OUT" 2>/dev/null || true
        sudo timeout -s INT 10 strace -c -f -p "$CLIPID" -o /home/vibe/recvwall/strace-cli.txt >/dev/null 2>>/home/vibe/recvwall/strace-err.txt || true
        log "== strace -c CLIENT (sender)"; cat /home/vibe/recvwall/strace-cli.txt >> "$OUT" 2>/dev/null || true
        # per-thread CPU snapshots
        for P in $SRVPID $CLIPID; do
            log "== /proc/$P/task utime+stime (ticks) t0"
            for t in /proc/$P/task/*; do awk '{print FILENAME, $14+$15}' $t/stat 2>/dev/null; done >> "$OUT"
        done
        sleep 8
        for P in $SRVPID $CLIPID; do
            log "== /proc/$P/task utime+stime (ticks) t0+8s"
            for t in /proc/$P/task/*; do awk '{print FILENAME, $14+$15}' $t/stat 2>/dev/null; done >> "$OUT"
        done
    else
        sudo perf record -F 397 -g -p "$SRVPID" -o /home/vibe/recvwall/perf-srv.data -- sleep 15 >/dev/null 2>&1 || true
        sudo perf record -F 397 -g -p "$CLIPID" -o /home/vibe/recvwall/perf-cli.data -- sleep 15 >/dev/null 2>&1 || true
        log "== perf report SERVER (receiver) — flat, top 60"
        sudo perf report -i /home/vibe/recvwall/perf-srv.data --stdio --no-children --percent-limit 0.4 2>/dev/null | head -90 >> "$OUT" || true
        log ""
        log "== perf report CLIENT (sender) — flat, top 40"
        sudo perf report -i /home/vibe/recvwall/perf-cli.data --stdio --no-children --percent-limit 0.5 2>/dev/null | head -60 >> "$OUT" || true
    fi
    # let the run finish or kill it — the numbers we need are captured
    wait $RUNNER 2>/dev/null || true
    grep -E "summary|CPU:" /home/vibe/recvwall/profrun-$1.out >> "$OUT" || true
    teardown
    ;;
quinn-recv)
    teardown
    sudo bash topo.sh up c1 --seed 42 >/dev/null 2>&1 || { log "TOPO-FAIL"; exit 1; }
    # quinn-perf: server in rp-srv (receiver of the upload), client in rp-cli
    sudo ip netns exec rp-srv "$QPERF" server --listen 10.77.0.2:4433 \
        > /home/vibe/recvwall/quinn-srv.log 2>&1 &
    sleep 1
    QSRV=$(pgrep -f "quinn-perf server" | head -1)
    log "quinn server pid=$QSRV"
    sudo ip netns exec rp-cli sh -c "timeout 60 $QPERF client raptorpath:4433 --ip 10.77.0.2 \
        --upload-size 8000000000 --download-size 0 --congestion bbr \
        --duration 45 --interval 45 --json /home/vibe/recvwall/quinn-cli.json \
        > /home/vibe/recvwall/quinn-cli.log 2>&1" &
    QCLI_RUNNER=$!
    sleep 6
    log "== SNMP/dev BEFORE (srv ns)"; counters rp-srv >> "$OUT" 2>&1
    S0=$(awk '{print $14+$15}' /proc/$QSRV/stat 2>/dev/null); T0=$(date +%s%N)
    sudo timeout -s INT 10 strace -c -f -p "$QSRV" -o /home/vibe/recvwall/strace-quinn-srv.txt >/dev/null 2>>/home/vibe/recvwall/strace-err.txt || true
    S1=$(awk '{print $14+$15}' /proc/$QSRV/stat 2>/dev/null); T1=$(date +%s%N)
    log "== SNMP/dev AFTER (srv ns, window $(( (T1-T0)/1000000 )) ms, srv ticks $S0 -> $S1)"; counters rp-srv >> "$OUT" 2>&1
    log "== strace -c QUINN SERVER (receiver)"; cat /home/vibe/recvwall/strace-quinn-srv.txt >> "$OUT" 2>/dev/null || true
    # perf the server for the flat receiver profile
    sudo perf record -F 397 -g -p "$QSRV" -o /home/vibe/recvwall/perf-quinn-srv.data -- sleep 12 >/dev/null 2>&1 || true
    log "== perf report QUINN SERVER — flat, top 40"
    sudo perf report -i /home/vibe/recvwall/perf-quinn-srv.data --stdio --no-children --percent-limit 0.5 2>/dev/null | head -60 >> "$OUT" || true
    wait $QCLI_RUNNER 2>/dev/null || true
    log "== quinn client result"
    sudo cat /home/vibe/recvwall/quinn-cli.json >> "$OUT" 2>/dev/null || true
    sudo tail -6 /home/vibe/recvwall/quinn-cli.log >> "$OUT" 2>/dev/null || true
    teardown
    ;;
*) echo "unknown phase $1" >&2; exit 1 ;;
esac
log "# done $(date -u +%FT%TZ)"
