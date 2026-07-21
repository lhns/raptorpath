#!/bin/bash
# meas/competitive-baseline master driver: env survey + bulk battery + realtime
# battery (rp arm via tail_matrix `ship`, competitors via compet_rt), both
# seeds. Run under nohup on the VM; progress lines to /home/vibe/compet/run.log.
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
OUT=/home/vibe/compet/run.log
mkdir -p /home/vibe/compet
: > "$OUT"
log() { echo "[$(date -u +%T)] $@" >> "$OUT"; }

log "competitive-baseline START"
# Environment survey (part of the record)
{
  echo "=== ENV SURVEY $(date -u +%FT%TZ)"
  echo "kernel: $(uname -a)"
  echo "tcp_cc_avail: $(sysctl -n net.ipv4.tcp_available_congestion_control)"
  echo "tcp_cc_default: $(sysctl -n net.ipv4.tcp_congestion_control)"
  echo "mptcp.enabled: $(sysctl -n net.mptcp.enabled 2>/dev/null || echo ABSENT)"
  sysctl net.mptcp 2>/dev/null || true
  echo "iperf3: $(iperf3 --version | head -1)"
  echo "python3: $(python3 --version)"
  echo "quinn checkout: $(git -C /home/vibe/quinn log --oneline -1 2>/dev/null)"
  echo "quinn-perf sha256: $(sha256sum /home/vibe/quinn/target/release/quinn-perf 2>/dev/null)"
  echo "msg_lat sha256: $(sha256sum /home/vibe/quinn/target/release/examples/msg_lat 2>/dev/null)"
  echo "raptorpath sha256: $(sha256sum /home/vibe/raptorpath/target/release/raptorpath)"
  echo "raptorpath commit: $(cat /home/vibe/raptorpath/COMMIT 2>/dev/null)"
} >> /home/vibe/compet/env.log 2>&1

for SEED in 42 7; do
    log "bulk battery seed=$SEED START"
    bash compet_battery.sh $SEED 8 >> "$OUT" 2>&1
    log "bulk battery seed=$SEED DONE"
    log "rt rp arm (tail_matrix ship) seed=$SEED c2 START"
    sudo env SEED=$SEED RWM_TM_ARMS="ship" bash tail_matrix.sh c2 8 \
        > /home/vibe/compet/tailrp-c2-s${SEED}.log 2>&1 || log "tail c2 s$SEED rc=$?"
    log "rt rp arm seed=$SEED c3 START"
    sudo env SEED=$SEED RWM_TM_ARMS="ship" bash tail_matrix.sh c3 8 \
        > /home/vibe/compet/tailrp-c3-s${SEED}.log 2>&1 || log "tail c3 s$SEED rc=$?"
    log "rt competitor arms seed=$SEED START"
    bash compet_rt.sh $SEED 8 >> "$OUT" 2>&1
    log "rt competitor arms seed=$SEED DONE"
done
log "competitive-baseline ALL DONE"
echo ALL-DONE
