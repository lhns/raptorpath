#!/bin/bash
# Adversarial L1 cells (ARC B1, goal-gate "Adversarial Cells (B1)"; the
# ADR-0068 prerequisite battery's substrate): the three link classes the
# clean netem rig cannot express, built as variants of the single-path
# rp-cli/rp-srv topology (same 10.77.0.x addressing as topo.sh, so every
# existing runner works unchanged).
#
#   delay-jitter (aggregation class — WiFi A-MPDU service bursts / LTE
#     scheduler+HARQ delay variation; the class ranges are the Sprout/LTE
#     measurement literature's tens-of-ms delay variability, see the
#     goal-gate pre-registration for the citations and their status):
#       jit0   netem delay 20ms                      (family control, no jitter)
#       jit5   netem delay 20ms  5ms 25%             (moderate WiFi-aggregation class)
#       jit15  netem delay 20ms 15ms 25%             (LTE scheduler class)
#       jit25  netem delay 20ms 25ms 25%             (heavy cellular class)
#     all at c2-class rate/loss (100mbit, GE 1.3% 50% data dir), jitter
#     BOTH directions (topo.sh convention), correlation 25%.
#     HONEST NOTE: netem jitter re-orders in-flow packets; real aggregation
#     mostly preserves order. Recorded in the pre-registration; both attack
#     the delay signal, the reorder excess makes this cell strictly harsher.
#
#   shallow-buffer:
#       shal8  bottleneck queue = 8 packets (vs netem's default 1000-pkt
#       deep buffer): cli0 root tbf (rate 100mbit) + child netem
#       limit 8 + GE 1.3% 50%. The queue lives in the CHILD netem (tbf
#       with a child enqueues there), so `limit 8` IS the buffer. All
#       propagation delay (10ms = c2-class RTT) moves to the ACK egress —
#       RTprop unchanged, the data-dir bottleneck holds only the queue.
#
#   policer (drop-without-queue):
#       pol100 token-bucket police at 100mbit, burst 16k (~12 pkt),
#       conform-exceed drop, on srv0 INGRESS — a rate ceiling that
#       presents pure loss with NO queueing-delay signal. Data-dir netem
#       carries delay only (5ms, limit 4000, must never drop — its
#       counter is recorded to prove 0); ACK dir netem delay 5ms rate
#       100mbit (deep) as usual. Offloads (gro/gso/tso) disabled on both
#       veth ends for shal8/pol100 so limits/policing act on wire-MTU
#       packets, not GSO super-packets.
#
#   c2ctl  byte-identical to topo.sh's c2 (the same-session clean control).
#
# Usage: sudo bash adv_cells.sh up <cell> [--seed N]
#        sudo bash adv_cells.sh down | status
#        sudo bash adv_cells.sh counters            (tc -s snapshot, parseable)
#        sudo bash adv_cells.sh validate <cell> [--seed N]
#          mechanism-liveness probes for the CELL itself (discipline item 1
#          applied to the substrate): idle ping RTT distribution, iperf3 UDP
#          overload (120M > the 100M ceiling) loss%, and ping-under-load
#          (the queue signature: deep cell inflates RTT under load, shallow
#          caps it, the policer never builds it).

set -euo pipefail
cd "$(dirname "$0")"
source ./lib.sh

IPERF_PORT=5210

adv_known() {
    case "$1" in c2ctl|jit0|jit5|jit15|jit25|shal8|pol100) return 0 ;; *) return 1 ;; esac
}

down() {
    for ns in "$NS_CLI" "$NS_SRV"; do
        guard_ns "$ns"
        ip netns del "$ns" 2>/dev/null || true
    done
    echo "adv topology down"
}

offloads_off() { # both veth ends at wire-MTU granularity (packet-count
                 # limits / per-packet policing must not see GSO aggregates)
    ip netns exec "$NS_CLI" ethtool -K cli0 gro off gso off tso off >/dev/null 2>&1 || true
    ip netns exec "$NS_SRV" ethtool -K srv0 gro off gso off tso off >/dev/null 2>&1 || true
}

up() {
    local cell="$1"; shift
    local seed=""
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --seed) seed="seed $2"; shift ;;
        esac
        shift
    done
    adv_known "$cell" || { echo "unknown adv cell: $cell" >&2; exit 1; }

    down >/dev/null 2>&1 || true
    ip netns add "$NS_CLI"
    ip netns add "$NS_SRV"
    ip link add cli0 netns "$NS_CLI" type veth peer name srv0 netns "$NS_SRV"
    ip -n "$NS_CLI" addr add 10.77.0.1/24 dev cli0
    ip -n "$NS_SRV" addr add 10.77.0.2/24 dev srv0
    ip -n "$NS_CLI" link set cli0 up
    ip -n "$NS_SRV" link set srv0 up
    ip -n "$NS_CLI" link set lo up
    ip -n "$NS_SRV" link set lo up

    case "$cell" in
        c2ctl)
            # byte-identical to topo.sh `up c2` (asymmetric loss, jitter both ways)
            # shellcheck disable=SC2086
            ip netns exec "$NS_CLI" tc qdisc add dev cli0 root netem \
                delay 5ms 3ms rate 100mbit loss gemodel 1.3% 50% $seed
            # shellcheck disable=SC2086
            ip netns exec "$NS_SRV" tc qdisc add dev srv0 root netem \
                delay 5ms 3ms rate 100mbit $seed
            ;;
        jit0|jit5|jit15|jit25)
            local J="${cell#jit}"
            local jarg=""
            [[ "$J" != "0" ]] && jarg="${J}ms 25%"
            # shellcheck disable=SC2086
            ip netns exec "$NS_CLI" tc qdisc add dev cli0 root netem \
                delay 20ms $jarg rate 100mbit loss gemodel 1.3% 50% $seed
            # shellcheck disable=SC2086
            ip netns exec "$NS_SRV" tc qdisc add dev srv0 root netem \
                delay 20ms $jarg rate 100mbit $seed
            ;;
        shal8)
            offloads_off
            # Bottleneck: tbf rate-shapes, the CHILD netem holds the (8-pkt)
            # queue + applies GE loss. burst ~1.5 MTU so the bucket cannot
            # hide a standing queue; tbf's own limit is irrelevant once the
            # child is attached (enqueue goes to the child).
            ip netns exec "$NS_CLI" tc qdisc add dev cli0 root handle 1: tbf \
                rate 100mbit burst 15140b limit 30280b
            # shellcheck disable=SC2086
            ip netns exec "$NS_CLI" tc qdisc add dev cli0 parent 1:1 handle 10: netem \
                limit 8 loss gemodel 1.3% 50% $seed
            # ALL propagation delay on the ACK egress (RTprop = 10ms, c2-class)
            ip netns exec "$NS_SRV" tc qdisc add dev srv0 root netem \
                delay 10ms rate 100mbit
            ;;
        pol100)
            offloads_off
            # Data dir: DELAY ONLY (no rate, deep limit — this stage must
            # never drop; its counter proves it).
            # shellcheck disable=SC2086
            ip netns exec "$NS_CLI" tc qdisc add dev cli0 root netem \
                delay 5ms limit 4000 $seed
            # ACK dir: c2-class delay+rate, deep (unpoliced).
            ip netns exec "$NS_SRV" tc qdisc add dev srv0 root netem \
                delay 5ms rate 100mbit
            # THE POLICER: srv0 ingress, token bucket at the link rate,
            # small burst, excess DROPPED with no queue and no delay.
            ip netns exec "$NS_SRV" tc qdisc add dev srv0 handle ffff: ingress
            ip netns exec "$NS_SRV" tc filter add dev srv0 parent ffff: \
                protocol all u32 match u32 0 0 \
                police rate 100mbit burst 16k mtu 65535 drop flowid :1
            ;;
    esac

    echo "adv topology up: $cell (${seed:-unseeded})"
    ip netns exec "$NS_CLI" ping -c 2 -i 0.2 -W 2 10.77.0.2 | tail -1
}

status() {
    ip netns list | grep '^rp-' || echo "(no rp-* namespaces)"
    for ns in "$NS_CLI" "$NS_SRV"; do
        if ip netns list | grep -q "^$ns"; then
            echo "--- $ns"
            ip -n "$ns" -br addr
            ip netns exec "$ns" tc qdisc show 2>/dev/null | grep -v noqueue || true
        fi
    done
}

counters() {
    # Parseable tc -s snapshot (the battery reads these BEFORE teardown):
    # every qdisc on both veths + the ingress police stats when present.
    echo "== CLI0 (data-dir egress: netem or tbf+netem bottleneck)"
    ip netns exec "$NS_CLI" tc -s qdisc show dev cli0 2>/dev/null || true
    echo "== SRV0 (ack-dir egress)"
    ip netns exec "$NS_SRV" tc -s qdisc show dev srv0 2>/dev/null || true
    echo "== SRV0-INGRESS (policer, when present)"
    ip netns exec "$NS_SRV" tc -s filter show dev srv0 parent ffff: 2>/dev/null || true
}

validate() {
    local cell="$1"; shift
    local seedargs=("$@")
    up "$cell" "${seedargs[@]}" >/dev/null
    echo "VALCELL $cell config:"
    status | sed 's/^/  /'

    # (1) idle RTT distribution: 30 pings — jitter cells must show
    # J-class mdev; shallow/policer must be tight at the base RTT.
    local idle
    idle=$(ip netns exec "$NS_CLI" ping -c 30 -i 0.2 -W 2 10.77.0.2 | tail -1)
    echo "VALCELL $cell idle_ping: $idle"

    # (2) UDP overload at 120M (> the 100M ceiling) with a concurrent ping:
    # loss% ~= the excess for a working ceiling; ping-under-load carries the
    # queue signature (deep: RTT inflates ~100ms; shal8: capped ~+1ms;
    # pol100: NO inflation — drop-without-queue, THE property under test).
    ip netns exec "$NS_SRV" iperf3 -s -D -p "$IPERF_PORT" --pidfile /tmp/adv-iperf-srv.pid
    sleep 0.3
    ip netns exec "$NS_CLI" iperf3 -u -c 10.77.0.2 -p "$IPERF_PORT" \
        -b 120M -t 10 -l 1200 --json >/tmp/adv-iperf.json 2>/dev/null &
    local ipid=$!
    sleep 2
    local under
    under=$(ip netns exec "$NS_CLI" ping -c 6 -i 1 -W 2 10.77.0.2 | tail -1 || true)
    wait "$ipid" 2>/dev/null || true
    pkill -F /tmp/adv-iperf-srv.pid 2>/dev/null || true
    python3 - "$cell" <<'EOF' || true
import json, sys
try:
    j = json.load(open("/tmp/adv-iperf.json"))
    s = j["end"]["sum"]
    print(f"VALCELL {sys.argv[1]} udp_overload_120M: rx={s.get('bits_per_second',0)/1e6:.1f}Mbit "
          f"loss={s.get('lost_percent',0):.1f}% jitter={s.get('jitter_ms',0):.2f}ms "
          f"lost={s.get('lost_packets',0)}/{s.get('packets',0)}")
except Exception as e:
    print(f"VALCELL {sys.argv[1]} udp_overload_120M: PARSE-FAIL {e}")
EOF
    echo "VALCELL $cell ping_under_load: ${under:-NO-REPLIES}"

    echo "VALCELL $cell counters_after_load:"
    counters | sed 's/^/  /'
    down >/dev/null
}

case "${1:-}" in
    up)       shift; up "$@" ;;
    down)     down ;;
    status)   status ;;
    counters) counters ;;
    validate) shift; validate "$@" ;;
    *) echo "usage: $0 up <cell> [--seed N] | down | status | counters | validate <cell> [--seed N]" >&2
       echo "cells: c2ctl jit0 jit5 jit15 jit25 shal8 pol100" >&2; exit 1 ;;
esac
