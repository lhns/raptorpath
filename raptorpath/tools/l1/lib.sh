#!/bin/bash
# Shared helpers for the L1 harness. See docs/l1-harness-plan.md.
#
# SAFETY: this file encodes the hard rules that protect SSH access to the
# test VM. All shaping happens on veth devices inside rp-* namespaces.

set -euo pipefail

# The VM's management interface — carries our SSH session. NEVER touched.
MGMT_IF="ens18"

NS_CLI="rp-cli"
NS_SRV="rp-srv"

# Refuse to operate on anything that could break remote access.
guard_dev() {
    local dev="$1"
    if [[ "$dev" == "$MGMT_IF" || "$dev" == "lo" ]]; then
        echo "REFUSED: will not touch device '$dev' (management/loopback)" >&2
        exit 1
    fi
}

guard_ns() {
    local ns="$1"
    if [[ "$ns" != rp-* ]]; then
        echo "REFUSED: namespace '$ns' is not rp-* prefixed" >&2
        exit 1
    fi
}

# Scenario table — identical parameterization to ADR-0051 / paper 2.4.
# Fields: rate one_way_ms jitter_ms ge_p ge_q
scenario_params() {
    case "$1" in
        c1|dc)       echo "1gbit   1   0  0.05 50" ;;
        c2|wifi)     echo "100mbit 5   3  1.3  50" ;;
        c3|lte)      echo "20mbit  20  5  2    40" ;;
        c4|sat)      echo "20mbit  100 10 3    30" ;;
        c5|badwifi)  echo "50mbit  5   3  5.3  30" ;;
        clean)       echo "100mbit 5   0  0    100" ;;
        # FEC-vs-ARQ crossover RTT sweep (feat/fec-arq-crossover): c2 loss/bw
        # (100mbit, GE 1.3/50 ≈ 2.5% mean loss) with jitter=0 so RTT is the ONLY
        # swept variable. one_way = RTT/2.  RTT ∈ {10,30,50,100,200} ms.
        c2r10)       echo "100mbit 5   0  1.3  50" ;;
        c2r30)       echo "100mbit 15  0  1.3  50" ;;
        c2r50)       echo "100mbit 25  0  1.3  50" ;;
        c2r100)      echo "100mbit 50  0  1.3  50" ;;
        c2r200)      echo "100mbit 100 0  1.3  50" ;;
        # Receiver-tail + FEC-favorable-regime sweep (feat/receiver-tail): the
        # SAME c2 pipe (100mbit, jitter=0) at RTT{100,200} but with HIGHER GE
        # loss. GE mean loss = p/(p+q); holding q=50 (burst structure) and
        # solving for p: 5% ⇒ p=2.63, 10% ⇒ p=5.56. FEC's advantage grows with
        # loss (ARQ retransmit-of-a-retransmit cascades; proactive FEC does not).
        c2r100l5)    echo "100mbit 50  0  2.63 50" ;;
        c2r100l10)   echo "100mbit 50  0  5.56 50" ;;
        c2r200l5)    echo "100mbit 100 0  2.63 50" ;;
        c2r200l10)   echo "100mbit 100 0  5.56 50" ;;
        *) echo "unknown scenario: $1" >&2; exit 1 ;;
    esac
}
