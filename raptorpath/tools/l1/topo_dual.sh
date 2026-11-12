#!/bin/bash
# Dual-path L1 topology (C7/C8): two veth pairs between rp-cli and rp-srv,
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
# Seeds are per leg. `--seed 42` gives leg A seed 42 and leg B seed 1042 —
# independent netem realizations. `--seed 42,42` puts one seed on both legs
# (the same loss realization = rho_loss +1 by construction at a symmetric
# cell). See lib.sh `leg_seed` for why the two are not comparable.

set -euo pipefail
cd "$(dirname "$0")"
source ./lib.sh
# The abort-cause witness. `set -E` makes the ERR trap inherit into `up()` —
# without it the trap is not taken inside a function. `set -E` changes no
# other behaviour: with no ERR trap installed it is inert, and the trap body
# only writes to the witness record.
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
    local child h=""
    child="$(scenario_child_qdisc "$scenario")"
    [[ -n "$child" ]] && h="handle 1:"
    # The netem words come from lib.sh `netem_args` (one builder for `shape` and `reshape`).
    # shellcheck disable=SC2046,SC2086
    ip netns exec "$ns" tc qdisc add dev "$dev" root $h netem $(netem_args "$scenario" "$seed")
    if [[ -n "$child" ]]; then
        ip netns exec "$ns" tc qdisc add dev "$dev" parent 1: handle 10: $child
    fi
    # The per-datagram loss-truth counter, ahead of netem on this data egress
    # (lib.sh `truth_counter_install`; netem's own counters are per skb).
    # `shape` is only ever called on the data direction.
    truth_counter_install "$ns" "$dev"
}

# A path event (status section 46): re-issue the FULL netem line of a live data egress with only the delay changed.
#   reshape <ns> <dev> <scenario> <seed> <delay_ms>      delay_ms is the absolute one-way delay
# It is `tc qdisc change` and nothing else. `add` or `replace` would drop the queue and reset the qdisc counters that `[TRUTH]`
# and the QDISC tail read, and the truth counter (clsact) is never touched.
reshape() {
    local ns="$1" dev="$2" scenario="$3" seed="${4:-}" delay_ms="${5:-}"
    guard_ns "$ns"
    guard_dev "$dev"
    [[ "$delay_ms" =~ ^[0-9]+$ ]] || { echo "REFUSED: reshape needs an integer delay in ms, got '$delay_ms'" >&2; return 1; }
    local child h=""
    child="$(scenario_child_qdisc "$scenario")"
    [[ -n "$child" ]] && h="handle 1:"
    # shellcheck disable=SC2046,SC2086
    ip netns exec "$ns" tc qdisc change dev "$dev" root $h netem $(netem_args "$scenario" "$seed" "$delay_ms")
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
    # Per-leg seeds: netem seeds its prng per qdisc, so one seed on both legs
    # of a symmetric cell makes the two paths' Gilbert-Elliott chains and
    # jitter draws the same realization indexed by packet (rho_loss = +1 by
    # construction). `leg_seed` derives one seed per leg from the base, and an
    # explicit comma list still pins them equal for the rho = +1 arm.
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

    # The active per-leg seeds, echoed: the derivation must be readable from
    # the run's own output and not only from this file (the `-q.txt` capture
    # carries them too, which is what the seed audit reads).
    echo "dual topology up: pathA=$scen_a pathB=$scen_b" \
         "seeds=[$(leg_seed "$seed" 0),$(leg_seed "$seed" 1)] (spec='${seed:-unset}')"
    # The sanity pings. Two ICMP packets with no retry across a
    # Gilbert-Elliott-lossy leg abort on the draw where both land in the bad
    # state, and `perf_rwm_c.sh` reads this script's exit code through
    # `aw_step topo_up`, so a lost draw aborts the invocation.
    #
    # So `aw_ping` (abort_witness.sh) retries, with its sizing
    # arithmetic: retry to at most `AW_PING_ATTEMPTS = 26` draws, accept the
    # first reply, which puts the false-abort probability at 2.0e-5 per leg at
    # the worst GE cell (c5). A genuinely dead leg still aborts.
    aw_ping "$NS_CLI" 10.77.0.2 pathA
    aw_ping "$NS_CLI" 10.78.0.2 pathB
}

case "${1:-}" in
    up) shift; up "$@" ;;
    down) down ;;
    shape) shift; shape "$@" ;;
    reshape) shift; reshape "$@" ;;
    oneway) scenario_one_way_ms "${2:?scenario}" ;;
    *) echo "usage: $0 up <scenA> <scenB> [--seed A[,B]] | down | shape <ns> <dev> <scen> <seed> | reshape <ns> <dev> <scen> <seed> <delay_ms> | oneway <scen>" >&2; exit 1 ;;
esac
