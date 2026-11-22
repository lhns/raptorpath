#!/bin/bash
# The real tunnel in the path: session R's standing-queue measurement and stage T. SOURCE it after omega_topo.sh and omega_battery.sh.
# Modelled on tun_bulk.sh: the underlay is the `clean` scenario (lib.sh scenario_params, read in a subshell so lib.sh's `set -e` never reaches
# this shell), raptorpath runs on both ends with the shipped defaults, TUN 10.99.0.1 / .2, a ping gate, three attempts.
# raptorpath is stopped by exact name (pkill -x raptorpath), and only when this session started it. Its binary is pinned by sha (ABORT-BUILD).

OMEGA_STARTED_RAPTORPATH=0
omega_stop_raptorpath() {
  [ "$OMEGA_STARTED_RAPTORPATH" = 1 ] || return 0
  pkill -TERM -x raptorpath 2>/dev/null || true
  local i
  for i in $(seq 1 30); do
    pgrep -x raptorpath >/dev/null 2>&1 || { OMEGA_STARTED_RAPTORPATH=0; return 0; }
    "${OMEGA_SLEEP:-sleep}" 0.1
  done
  pkill -KILL -x raptorpath 2>/dev/null || true
  OMEGA_STARTED_RAPTORPATH=0
}

tunnel_up() { # bin hint  -- 0 up; 1 = TUN-BRINGUP-FAIL after three attempts (stage T is skipped, not the session)
  local bin="$1" hint="$2" attempt i up=0 rate one_way jitter ge_p ge_q fwd
  read -r rate one_way jitter ge_p ge_q <<< "$( ( source "$HERE/lib.sh" >/dev/null 2>&1; scenario_params clean ) )"
  fwd="$( ( source "$HERE/lib.sh" >/dev/null 2>&1; rwm_forward_env ) )"
  for attempt in 1 2 3; do
    omega_stop_raptorpath; topo_down
    ns_add $OMEGA_NS_CLI && ns_add $OMEGA_NS_SRV || return 1
    nsip $OMEGA_NS_CLI link add cli0 type veth peer name srv0 netns $OMEGA_NS_SRV || return 1
    nsip $OMEGA_NS_CLI addr add 10.77.0.1/24 dev cli0
    nsip $OMEGA_NS_SRV addr add 10.77.0.2/24 dev srv0
    nsip $OMEGA_NS_CLI link set cli0 up; nsip $OMEGA_NS_SRV link set srv0 up
    nsip $OMEGA_NS_CLI link set lo up; nsip $OMEGA_NS_SRV link set lo up
    nsx $OMEGA_NS_CLI tc qdisc replace dev cli0 root netem delay "${one_way}ms" rate "$rate" || return 1
    nsx $OMEGA_NS_SRV tc qdisc replace dev srv0 root netem delay "${one_way}ms" rate "$rate" || return 1
    OMEGA_STARTED_RAPTORPATH=1
    # shellcheck disable=SC2086
    omega_bg $OMEGA_NS_SRV "$RUN/tun-s.log" env -u RWM_EST_CADENCE -u RWM_POOL_ANCHOR -u RWM_EMIT_BATCH -u RWM_EMIT_BURST $fwd "$bin" run --server --bind 10.77.0.2:7000 \
      --tun-name rpsrv0 --tun-addr 10.99.0.2/24 --protocol-hint "$hint"
    "${OMEGA_SLEEP:-sleep}" 2
    # shellcheck disable=SC2086
    omega_bg $OMEGA_NS_CLI "$RUN/tun-c.log" env -u RWM_EST_CADENCE -u RWM_POOL_ANCHOR -u RWM_EMIT_BATCH -u RWM_EMIT_BURST $fwd "$bin" run --peer 10.77.0.2:7000 --bind 10.77.0.1:0 \
      --tun-name rpcli0 --tun-addr 10.99.0.1/24 --protocol-hint "$hint"
    for i in $(seq 1 20); do
      nsx $OMEGA_NS_CLI ping -c1 -W1 10.99.0.2 >/dev/null 2>&1 && { up=1; break; }
      "${OMEGA_SLEEP:-sleep}" 1
    done
    [ "$up" = 1 ] && break
    echo "TUN-BRINGUP-RETRY attempt=$attempt"
  done
  [ "$up" = 1 ] || { echo "TUN-BRINGUP-FAIL"; return 1; }
  echo "TUNMTU cli=$(nsx $OMEGA_NS_CLI cat /sys/class/net/rpcli0/mtu 2>/dev/null || echo -) srv=$(nsx $OMEGA_NS_SRV cat /sys/class/net/rpsrv0/mtu 2>/dev/null || echo -)"
  echo "TUNQLEN cli=$(nsx $OMEGA_NS_CLI cat /sys/class/net/rpcli0/tx_queue_len 2>/dev/null || echo -) default_qdisc=$(nsx $OMEGA_NS_CLI sysctl -n net.core.default_qdisc 2>/dev/null || echo -)"
}

# Stage T: rp-app behind rp-srv. The inner flows run rp-cli -> rp-app through the tunnel; the reorderer sits on srv1 (after TUN delivery).
tunnel_app_up() {
  topo_up_app || return 1
  nsip $OMEGA_NS_CLI route add 10.88.0.0/24 via 10.99.0.2 dev rpcli0 || return 1
}

# Session R: the standing queue through the real tunnel (registration 11.1). One long-lived inner CUBIC flow, then F = 8, each OMEGA_TUNQ_S seconds
# (default 90). srtt - minrtt every 100 ms from `ss -tin`; B_t = g_tun * median over the last 60 s of the F = 8 phase. Writes $RUN/QT.json.
omega_tunnel_phase() { # phase nflows
  local ph="$1" nf="$2" cpid spid dur="${OMEGA_TUNQ_S:-90}" t
  : > "$RUN/tunq-$ph.ss"
  omega_bg $OMEGA_NS_SRV "$RUN/tunq-$ph.srv" iperf3 -s -1 -p 5301; spid="$OMEGA_LASTPID"
  omega_sleep "${OMEGA_SETTLE_S:-0.5}"
  omega_bg $OMEGA_NS_CLI "$RUN/tunq-$ph.json" iperf3 -c 10.99.0.2 -p 5301 -P "$nf" -J -t "$dur"; cpid="$OMEGA_LASTPID"
  t="$(omega_now)"
  while :; do
    nsx $OMEGA_NS_CLI ss -tin dst 10.99.0.2 2>/dev/null | tr '\n' ' ' \
      | sed -n "s/.*[ ]rtt:\([0-9.]*\)\/[0-9.]*.*minrtt:\([0-9.]*\).*/$(omega_now) \1 \2/p" >> "$RUN/tunq-$ph.ss"
    nsx $OMEGA_NS_CLI tc -s qdisc show dev rpcli0 > "$RUN/tunq-$ph.tc-rpcli0" 2>/dev/null
    nsx $OMEGA_NS_CLI tc -s qdisc show dev cli0 > "$RUN/tunq-$ph.tc-cli0" 2>/dev/null
    kill -0 "$cpid" 2>/dev/null || break
    omega_sleep 0.1
    if [ "$(awk -v n="$(omega_now)" -v t="$t" -v d="$dur" 'BEGIN{print (n - t > d + 10) ? 1 : 0}')" = 1 ]; then kill -TERM "$cpid" 2>/dev/null; break; fi
  done
  wait "$cpid" 2>/dev/null
  omega_stop_pid "$spid"
}

omega_tunnel_measure() { # -> 0 measured, 1 skipped or failed (reported; R continues)
  [ -n "${OMEGA_BIN:-}" ] || { omega_note "TUNNEL-QUEUE-SKIPPED no OMEGA_BIN: B_t is not measured; Q_t uses a placeholder and the amendment must pin it"; return 1; }
  local sha
  sha="$(sha256sum "$OMEGA_BIN" | cut -d' ' -f1)"
  if [ -n "${OMEGA_BIN_SHA:-}" ] && [ "$sha" != "$OMEGA_BIN_SHA" ]; then omega_abort ABORT-BUILD "raptorpath sha $sha differs from the pin $OMEGA_BIN_SHA"; fi
  omega_note "TUNSHA $sha"
  tunnel_up "$OMEGA_BIN" auto || { omega_note "TUN-BRINGUP-FAIL the standing queue is not measured"; return 1; }
  sysctl_apply $OMEGA_NS_CLI >/dev/null && sysctl_apply $OMEGA_NS_SRV >/dev/null || omega_abort ABORT-SYSCTL "stack D in the tunnel namespaces"
  nsx $OMEGA_NS_CLI ip addr show dev rpcli0 >/dev/null 2>&1 || true
  omega_tunnel_phase single 1
  omega_tunnel_phase f8 8
  $PY "$HERE/omega_parse.py" tunq "$RUN/tunq-f8.ss" "$RUN/tunq-f8.json" "${OMEGA_TUN_CEIL_MBIT:-100}" > "$RUN/QT.json" || true
  $PY "$HERE/omega_parse.py" tunq "$RUN/tunq-single.ss" "$RUN/tunq-single.json" "${OMEGA_TUN_CEIL_MBIT:-100}" > "$RUN/QT-single.json" || true
  omega_note "QT samples=$(wc -l < "$RUN/tunq-f8.ss") $(cat "$RUN/QT.json" 2>/dev/null)"
  omega_stop_raptorpath
  return 0
}
