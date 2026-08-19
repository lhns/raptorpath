#!/bin/bash
# Dual-path L1 topology (C7/C8): TWO veth pairs between rp-cli and rp-srv,
# each shaped independently. Configures MPTCP endpoints so kernel MPTCP
# (and raptorpath's two paths) can use both.
#
#   [rp-cli] cli0 10.77.0.1 ─ pathA ─ 10.77.0.2 srv0 [rp-srv]
#            cli1 10.78.0.1 ─ pathB ─ 10.78.0.2 srv1
#
# Usage: sudo bash topo_dual.sh up <scenarioA> <scenarioB> [--seed A[,B]]
#        sudo bash topo_dual.sh down
#   C7 = up c2 c2      C8 = up c2 c3
#
# SEEDS ARE PER LEG (2026-08-19). `--seed 42` gives leg A seed 42 and leg B
# seed 1042 — INDEPENDENT netem realizations. `--seed 42,42` reproduces the
# pre-2026-08-19 behaviour exactly (one seed on both legs = the SAME loss
# realization = rho_loss +1 by construction at a symmetric cell). See the
# HARNESS ERA note in lib.sh for the era-comparability consequence; it is
# load-bearing for any comparison that spans the date.

set -euo pipefail
cd "$(dirname "$0")"
source ./lib.sh

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

    # Path A: loss on data direction only (matching topo.sh), delay both ways.
    #
    # PER-LEG SEEDS. These two lines used to pass the SAME `$seed` to both
    # legs. netem seeds its prng per qdisc, so at a symmetric cell that made
    # the two paths' Gilbert-Elliott chains and jitter draws the SAME
    # realization indexed by packet — rho_loss = +1 BY CONSTRUCTION, at
    # exactly the cells where pooling wins, for the whole previous harness era
    # (goal-gate "Eppen's Condition at c8" §2, THE SEED AUDIT; NEEDS-MORE 2).
    # `leg_seed` derives one seed per leg from the base, and an explicit
    # comma list still pins them equal for the rho = +1 arm.
    shape "$NS_CLI" cli0 "$scen_a" "$(leg_seed "$seed" 0)"
    shape "$NS_CLI" cli1 "$scen_b" "$(leg_seed "$seed" 1)"
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

    # The ACTIVE per-leg seeds, echoed: the derivation must be readable from
    # the run's own output and not only from this file (the `-q.txt` capture
    # carries them too, which is what the seed audit reads).
    echo "dual topology up: pathA=$scen_a pathB=$scen_b" \
         "seeds=[$(leg_seed "$seed" 0),$(leg_seed "$seed" 1)] (spec='${seed:-unset}')"
    ip netns exec "$NS_CLI" ping -c 2 -i 0.2 -W 2 10.77.0.2 | tail -1
    ip netns exec "$NS_CLI" ping -c 2 -i 0.2 -W 2 10.78.0.2 | tail -1
}

case "${1:-}" in
    up) shift; up "$@" ;;
    down) down ;;
    *) echo "usage: $0 up <scenA> <scenB> [--seed A[,B]] | down" >&2; exit 1 ;;
esac
