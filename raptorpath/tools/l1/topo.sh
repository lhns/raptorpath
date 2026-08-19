#!/bin/bash
# L1 topology: two namespaces joined by a veth pair with netem shaping.
#
#   [rp-cli] cli0 (10.77.0.1) ── veth ── (10.77.0.2) srv0 [rp-srv]
#
# netem: delay/jitter/rate on BOTH egresses (one-way each => full RTT);
# Gilbert-Elliott loss on the DATA egress (cli0) only, ACK path clean —
# matching the L0 gate's forward-loss model. --symmetric adds loss on
# both directions.
#
# Usage: sudo bash topo.sh up <scenario> [--symmetric] [--seed N]
#        sudo bash topo.sh down
#        sudo bash topo.sh status

set -euo pipefail
cd "$(dirname "$0")"
source ./lib.sh
# THE ABORT-CAUSE WITNESS — see topo_dual.sh for why `set -E` is required (the
# ERR trap is not inherited into `up()` without it) and why it is otherwise
# inert.
set -E
source ./abort_witness.sh
trap 'aw_err_trap "$?" "$LINENO" "$BASH_COMMAND"' ERR

down() {
    for ns in "$NS_CLI" "$NS_SRV"; do
        guard_ns "$ns"
        ip netns del "$ns" 2>/dev/null || true
    done
    echo "topology down"
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

up() {
    local scenario="$1"; shift
    local symmetric=0 seed=""
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --symmetric) symmetric=1 ;;
            --seed) seed="seed $2"; shift ;;
        esac
        shift
    done

    read -r rate one_way jitter ge_p ge_q <<< "$(scenario_params "$scenario")"
    local jit=""
    [[ "$jitter" != "0" ]] && jit="${jitter}ms"
    local loss=""
    [[ "$ge_p" != "0" ]] && loss="loss gemodel ${ge_p}% ${ge_q}%"

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

    # Data direction (cli -> srv): delay + rate + GE loss
    # shellcheck disable=SC2086
    ip netns exec "$NS_CLI" tc qdisc add dev cli0 root netem \
        delay "${one_way}ms" $jit rate "$rate" $loss $seed

    # ACK direction (srv -> cli): delay + rate (+ loss if --symmetric)
    local rloss=""
    [[ $symmetric -eq 1 ]] && rloss="$loss"
    # shellcheck disable=SC2086
    ip netns exec "$NS_SRV" tc qdisc add dev srv0 root netem \
        delay "${one_way}ms" $jit rate "$rate" $rloss $seed

    echo "topology up: $scenario (rate=$rate, one_way=${one_way}ms, jitter=${jitter}ms, GE p=${ge_p}% q=${ge_q}%, symmetric=$symmetric)"
    # Sanity ping (also warms ARP). RECORDED, with the same exit semantics —
    # and, as in topo_dual.sh, it is the LAST statement of `up()`, so its
    # failure cannot leave an incomplete topology behind.
    aw_ping "$NS_CLI" 10.77.0.2 single
}

case "${1:-}" in
    up) shift; up "$@" ;;
    down) down ;;
    status) status ;;
    *) echo "usage: $0 up <scenario> [--symmetric] [--seed N] | down | status" >&2; exit 1 ;;
esac
