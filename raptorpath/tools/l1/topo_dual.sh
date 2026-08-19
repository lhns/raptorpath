#!/bin/bash
# Dual-path L1 topology (C7/C8): TWO veth pairs between rp-cli and rp-srv,
# each shaped independently. Configures MPTCP endpoints so kernel MPTCP
# (and raptorpath's two paths) can use both.
#
#   [rp-cli] cli0 10.77.0.1 ─ pathA ─ 10.77.0.2 srv0 [rp-srv]
#            cli1 10.78.0.1 ─ pathB ─ 10.78.0.2 srv1
#
# Usage: sudo bash topo_dual.sh up <scenarioA> <scenarioB> [--seed N]
#        sudo bash topo_dual.sh down
#   C7 = up c2 c2      C8 = up c2 c3

set -euo pipefail
cd "$(dirname "$0")"
source ./lib.sh
# THE ABORT-CAUSE WITNESS. `set -E` makes the ERR trap inherit into `up()` —
# without it the trap is not taken inside a function and every failure in this
# file would still be attributed to nothing. `set -E` changes NO other
# behaviour: with no ERR trap installed it is inert, and the trap body only
# writes to the witness record.
set -E
source ./abort_witness.sh
trap 'aw_err_trap "$?" "$LINENO" "$BASH_COMMAND"' ERR

down() {
    for ns in "$NS_CLI" "$NS_SRV"; do
        guard_ns "$ns"
        ip netns del "$ns" 2>/dev/null || true
    done
    echo "dual topology down"
}

shape() { # ns dev scenario seed
    local ns="$1" dev="$2" scenario="$3" seed="${4:-}"
    read -r rate one_way jitter ge_p ge_q <<< "$(scenario_params "$scenario")"
    local jit=""
    [[ "$jitter" != "0" ]] && jit="${jitter}ms"
    local loss=""
    [[ "$ge_p" != "0" ]] && loss="loss gemodel ${ge_p}% ${ge_q}%"
    local sd=""
    [[ -n "$seed" ]] && sd="seed $seed"
    # shellcheck disable=SC2086
    ip netns exec "$ns" tc qdisc add dev "$dev" root netem \
        delay "${one_way}ms" $jit rate "$rate" $loss $sd
}

up() {
    local scen_a="$1" scen_b="$2"; shift 2
    local seed=""
    [[ "${1:-}" == "--seed" ]] && seed="$2"

    down >/dev/null 2>&1 || true
    ip netns add "$NS_CLI"
    ip netns add "$NS_SRV"

    ip link add cli0 netns "$NS_CLI" type veth peer name srv0 netns "$NS_SRV"
    ip link add cli1 netns "$NS_CLI" type veth peer name srv1 netns "$NS_SRV"
    ip -n "$NS_CLI" addr add 10.77.0.1/24 dev cli0
    ip -n "$NS_SRV" addr add 10.77.0.2/24 dev srv0
    ip -n "$NS_CLI" addr add 10.78.0.1/24 dev cli1
    ip -n "$NS_SRV" addr add 10.78.0.2/24 dev srv1
    for l in cli0 cli1 lo; do ip -n "$NS_CLI" link set "$l" up; done
    for l in srv0 srv1 lo; do ip -n "$NS_SRV" link set "$l" up; done

    # Path A: loss on data direction only (matching topo.sh), delay both ways
    shape "$NS_CLI" cli0 "$scen_a" "$seed"
    shape "$NS_CLI" cli1 "$scen_b" "$seed"
    # Reverse (ACK) direction: delay/rate only — build a lossless variant by
    # reusing shape with loss stripped via the 'clean-<scenario>' trick:
    read -r rate_a ow_a jit_a _ _ <<< "$(scenario_params "$scen_a")"
    read -r rate_b ow_b jit_b _ _ <<< "$(scenario_params "$scen_b")"
    ja=""; [[ "$jit_a" != "0" ]] && ja="${jit_a}ms"
    jb=""; [[ "$jit_b" != "0" ]] && jb="${jit_b}ms"
    # shellcheck disable=SC2086
    ip netns exec "$NS_SRV" tc qdisc add dev srv0 root netem delay "${ow_a}ms" $ja rate "$rate_a"
    # shellcheck disable=SC2086
    ip netns exec "$NS_SRV" tc qdisc add dev srv1 root netem delay "${ow_b}ms" $jb rate "$rate_b"

    # MPTCP: allow extra subflows and announce the second address
    for ns in "$NS_CLI" "$NS_SRV"; do
        ip netns exec "$ns" sysctl -q net.mptcp.enabled=1
        ip netns exec "$ns" ip mptcp limits set subflow 2 add_addr_accepted 2
    done
    ip netns exec "$NS_CLI" ip mptcp endpoint add 10.78.0.1 dev cli1 subflow
    ip netns exec "$NS_SRV" ip mptcp endpoint add 10.78.0.2 dev srv1 signal

    echo "dual topology up: pathA=$scen_a pathB=$scen_b"
    # THE "TOPO-PING". Same two pings, same exit semantics (`aw_ping` re-returns
    # the ping's status, so `set -e` still aborts here exactly as before) — but
    # the rc and the output are now RECORDED. Note what the position of these
    # two lines already proves: they are the LAST statements of `up()`, so a
    # failure here leaves a COMPLETE topology behind, and `perf_rwm_c.sh` does
    # not read this script's exit code at all. The abort class named after them
    # cannot have been caused by them.
    aw_ping "$NS_CLI" 10.77.0.2 pathA
    aw_ping "$NS_CLI" 10.78.0.2 pathB
}

case "${1:-}" in
    up) shift; up "$@" ;;
    down) down ;;
    *) echo "usage: $0 up <scenA> <scenB> [--seed N] | down" >&2; exit 1 ;;
esac
