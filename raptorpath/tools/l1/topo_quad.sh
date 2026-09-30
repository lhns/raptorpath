#!/bin/bash
# Quad-path L1 topology (C9/C9H): four veth pairs between rp-cli and rp-srv,
# each shaped independently. Follows topo_dual.sh's shape exactly — same
# namespaces, same addressing stride, same reverse-direction treatment, same
# MPTCP endpoint configuration — widened from 2 legs to 4.
#
#   [rp-cli] cli0 10.77.0.1 ─ pathA ─ 10.77.0.2 srv0 [rp-srv]
#            cli1 10.78.0.1 ─ pathB ─ 10.78.0.2 srv1
#            cli2 10.79.0.1 ─ pathC ─ 10.79.0.2 srv2
#            cli3 10.80.0.1 ─ pathD ─ 10.80.0.2 srv3
#
# Usage: sudo bash topo_quad.sh up <scenA> <scenB> <scenC> <scenD> [--seed S]
#        sudo bash topo_quad.sh down
#   C9  = up c2 c2 c2 c2     the symmetric quad  (4 × the c2-class leg)
#   C9H = up c2 c2 c3 c3     the heterogeneous quad (2 × c2 + 2 × c3)
#
# The bench twin: `tests/store_cap_sf_bench.rs`'s `c7x4` is
# `vec![C2, C2, C2, C2]` with `C2 = (10_400 sym/s, RTprop 0.008 s, GE loss
# 0.013, GE persistence 0.50)` and `MAX_PATHS = 4`. c9 is the wire twin of
# that simulated geometry: the same four-way symmetric c2 leg, shaped by netem
# instead of by the bench's link model. The two must never be pooled — the
# bench has no kernel, no QUIC and no real scheduler — but the bench's numbers
# are the prediction the wire is read against. c9h has no bench twin.
#
# Seeds are per leg (see lib.sh `leg_seed`). `--seed 42` gives
# 42/1042/2042/3042 — four independent netem realizations. `--seed
# 42,42,42,42` pins them equal, i.e. the rho_loss = +1 arm.

set -euo pipefail
cd "$(dirname "$0")"
source ./lib.sh
# The abort-cause witness: with the `ERR` trap a quad failure names the exact
# failing line inside `up()`, not just `topo_up` plus a stderr blob — worth
# more at four legs, with twice as many identical-looking statements.
# `set -E` makes the trap inherit into `up()` (without it the trap is not taken
# inside a function); it changes nothing else, and the trap body only writes to
# the witness record.
set -E
source ./abort_witness.sh
trap 'aw_err_trap "$?" "$LINENO" "$BASH_COMMAND"' ERR

# The four legs, as parallel arrays. One definition, read by both `up` and
# `down`, so a fifth leg is a single edit and cannot be added to one half of
# the script only.
CLI_DEVS=(cli0 cli1 cli2 cli3)
SRV_DEVS=(srv0 srv1 srv2 srv3)
CLI_ADDRS=(10.77.0.1 10.78.0.1 10.79.0.1 10.80.0.1)
SRV_ADDRS=(10.77.0.2 10.78.0.2 10.79.0.2 10.80.0.2)
NLEGS=4

down() {
    for ns in "$NS_CLI" "$NS_SRV"; do
        guard_ns "$ns"
        ip netns del "$ns" 2>/dev/null || true
    done
    echo "quad topology down"
}

shape() { # ns dev scenario seed
    local ns="$1" dev="$2" scenario="$3" seed="${4:-}"
    # The guards, on every shaping call. `guard_dev` refuses the management
    # interface (ens18 — it carries the SSH session) and loopback; `guard_ns`
    # refuses any namespace that is not rp-* prefixed. topo_dual.sh relies on
    # its device names being literals; here they come out of an array, which
    # is exactly the thing a later edit can widen wrongly.
    guard_ns "$ns"
    guard_dev "$dev"
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
    # The per-datagram loss-truth counter, ahead of netem on this data egress
    # (lib.sh `truth_counter_install`; netem's own counters are per skb).
    # `shape` is only ever called on the data direction.
    truth_counter_install "$ns" "$dev"
}

up() {
    local -a scen=("$1" "$2" "$3" "$4"); shift 4
    local seed=""
    [[ "${1:-}" == "--seed" ]] && seed="$2"

    down >/dev/null 2>&1 || true
    ip netns add "$NS_CLI"
    ip netns add "$NS_SRV"

    local i
    for ((i = 0; i < NLEGS; i++)); do
        guard_dev "${CLI_DEVS[$i]}"
        guard_dev "${SRV_DEVS[$i]}"
        ip link add "${CLI_DEVS[$i]}" netns "$NS_CLI" \
            type veth peer name "${SRV_DEVS[$i]}" netns "$NS_SRV"
        ip -n "$NS_CLI" addr add "${CLI_ADDRS[$i]}/24" dev "${CLI_DEVS[$i]}"
        ip -n "$NS_SRV" addr add "${SRV_ADDRS[$i]}/24" dev "${SRV_DEVS[$i]}"
    done
    for l in "${CLI_DEVS[@]}" lo; do ip -n "$NS_CLI" link set "$l" up; done
    for l in "${SRV_DEVS[@]}" lo; do ip -n "$NS_SRV" link set "$l" up; done

    # Data direction: loss + delay + rate, one independent netem seed per leg.
    for ((i = 0; i < NLEGS; i++)); do
        shape "$NS_CLI" "${CLI_DEVS[$i]}" "${scen[$i]}" "$(leg_seed "$seed" "$i")"
    done
    # Reverse (ACK) direction: delay/rate only, no loss and no seed — as
    # topo_dual.sh does it. The kernel draws its own prng seed there, so the
    # `SRV*` captures read random 64-bit values; that is the seed audit's own
    # control.
    for ((i = 0; i < NLEGS; i++)); do
        read -r rate ow jit_ms _ _ <<< "$(scenario_params "${scen[$i]}")"
        j=""; [[ "$jit_ms" != "0" ]] && j="${jit_ms}ms"
        guard_dev "${SRV_DEVS[$i]}"
        # shellcheck disable=SC2086
        ip netns exec "$NS_SRV" tc qdisc add dev "${SRV_DEVS[$i]}" root netem \
            delay "${ow}ms" $j rate "$rate"
    done

    # MPTCP: allow the extra subflows and announce the extra addresses. The
    # dual sets `subflow 2 add_addr_accepted 2` for its one extra leg; a quad
    # has three extra legs, so the limits and the endpoint count widen with
    # NLEGS rather than being hard-coded (a widened array beside an un-widened
    # bound is the defect class to avoid).
    local extra=$((NLEGS))
    for ns in "$NS_CLI" "$NS_SRV"; do
        ip netns exec "$ns" sysctl -q net.mptcp.enabled=1
        ip netns exec "$ns" ip mptcp limits set subflow "$extra" \
            add_addr_accepted "$extra"
    done
    for ((i = 1; i < NLEGS; i++)); do
        ip netns exec "$NS_CLI" ip mptcp endpoint add "${CLI_ADDRS[$i]}" \
            dev "${CLI_DEVS[$i]}" subflow
        ip netns exec "$NS_SRV" ip mptcp endpoint add "${SRV_ADDRS[$i]}" \
            dev "${SRV_DEVS[$i]}" signal
    done

    # The active per-leg seeds, echoed: the derivation must be readable from
    # the run's own output and not only from lib.sh.
    local seeds=""
    for ((i = 0; i < NLEGS; i++)); do
        seeds="${seeds}${seeds:+,}$(leg_seed "$seed" "$i")"
    done
    echo "quad topology up: paths=${scen[*]} seeds=[$seeds] (spec='${seed:-unset}')"
    # The sanity ping, recorded, on all four legs. `aw_ping` retries to at
    # most `AW_PING_ATTEMPTS = 26` draws and accepts the first reply. At the
    # worst GE cell (c5, pi_bad = 0.1501, persistence 0.70) a false abort is
    # 2.0e-5 per leg and 8.05e-5 for the whole four-legged invocation, under
    # 1e-4 (sizing arithmetic in `abort_witness.sh`). A leg with no namespace,
    # no address or no route still aborts, and fast.
    for ((i = 0; i < NLEGS; i++)); do
        aw_ping "$NS_CLI" "${SRV_ADDRS[$i]}" "path$i"
    done
}

case "${1:-}" in
    up)
        shift
        [[ $# -ge 4 ]] || {
            echo "usage: $0 up <scenA> <scenB> <scenC> <scenD> [--seed S] | down" >&2
            exit 1
        }
        up "$@"
        ;;
    down) down ;;
    *)
        echo "usage: $0 up <scenA> <scenB> <scenC> <scenD> [--seed S] | down" >&2
        exit 1
        ;;
esac
