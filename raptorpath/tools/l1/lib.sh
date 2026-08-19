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

# ── GATE FORWARDING (goal-gate "Gate-Forwarding Audit", 2026-08-09) ──────
#
# THE ONE list of `RWM_*` knobs the harness forwards to the binary, and the
# ONE function that turns it into an `env` prefix. Every driver that launches
# the binary sources this file and passes `$(rwm_forward_env)`.
#
# WHY THIS EXISTS. Before this, each driver hand-rolled its own allowlist
# (`perf_rwm_c.sh` had 78 lines of `[[ -n "${RWM_X:-}" ]] && TENV="$TENV ..."`)
# and the ack-merge battery discovered `RWM_ACK_MERGE` had never been added to
# it. That battery was nevertheless VALID — the audit MEASURED (PROBE 0,
# 2026-08-09) that `sudo env VAR=… → bash driver → ip netns exec ns env $TENV`
# delivers the var by plain process-environment INHERITANCE whether or not the
# allowlist names it. So the allowlists were, and are, load-bearing for
# NOTHING; they only created a false impression of explicitness while 12
# engine gates silently sat outside them. This function makes the forwarding
# TOTAL and EXPLICIT so the impression matches the mechanism.
#
# ENFORCEMENT: `raptorpath`'s `gate_forwarding_list_covers_the_engine_surface`
# test parses THIS array and fails if any `RWM_*` the engine reads is missing.
# Adding a gate to the engine without adding it here fails the suite.
RWM_FORWARD=(
    RWM_ACKDIAG RWM_ACKDIAG_WINDOW_US
    RWM_ACK_MERGE RWM_ANCHOR_HYGIENE RWM_ASTAR_ANCHOR RWM_CC_PACE
    RWM_CC_PACE_HR RWM_CHARGE_RECOVERY RWM_CLOCK_GAP RWM_CODED_SRC RWM_COLD_PLACE RWM_COPA_COMPETE
    RWM_COMPOSED_CAP RWM_COPA_DELTA RWM_COPA_FEED RWM_COPA_WIRE RWM_CPUPROF
    RWM_DERIVED_SWEEP RWM_DIAG
    RWM_EMIT_BATCH RWM_EMIT_BURST RWM_EST_CADENCE RWM_FDIAG
    RWM_FLOOR_BOUND RWM_GEN RWM_GEN_INFLIGHT RWM_GEN_PIPE
    RWM_GEN_R RWM_GEN_RATE RWM_GEN_RATE_FLOOR RWM_HONEST_ANCHOR
    RWM_HONEST_CAP RWM_HONEST_K
    RWM_INFL_BDP RWM_INFL_CAP RWM_L0_NETEM RWM_L0_SEED
    RWM_LATE_BRAKE RWM_LOSS_SENT_TRUTH
    RWM_MIN_R RWM_MSTAR_ANCHOR RWM_MTU_FLOOR RWM_NO_REACTIVE
    RWM_OOO_RETAIN RWM_PATIENCE_DERIVED RWM_PERCAP_GUARD RWM_PERF_TIMEOUT_S
    RWM_PFRAC RWM_PIPELINE RWM_PLACE_SLACK RWM_PLACE_T
    RWM_PLAIN_RS RWM_POOL_ANCHOR RWM_POOL_DELIV RWM_PROACTIVE_PACER
    RWM_QUIC_CC RWM_RDIAG RWM_REACT_CAP RWM_REASM_BDP
    RWM_RECOV_MP RWM_RECOV_MP_LAW RWM_RECOV_MP_LIVE RWM_RECOV_SP
    RWM_RELEASE_1TO1
    RWM_REPAIR_WAIT RWM_REPORT_GENS RWM_RSTAR_TAIL RWM_RS_ATTR
    RWM_RS_TRACE RWM_SIDLE_DERIVED RWM_STORE
    RWM_STORE_BOOT RWM_STORE_BORROW RWM_STORE_CAPW RWM_STORE_CAP_UNIFIED RWM_STORE_GAIN
    RWM_STORE_PATHS RWM_STORE_PATH_POOL RWM_STORE_PERCAP RWM_STORE_SACK_RELEASE
    RWM_SUM_CAP RWM_DELTA_CAP RWM_RACK_CLOCKS RWM_RACK_REO_MULT RWM_QUANTILE_CLOCKS
    RWM_TAPER_R RWM_THREE_TERM RWM_TRACE RWM_UNIFIED RWM_UNIFIED_SHED
    RWM_WALLDIAG RWM_WINDOW RWM_WIN_DECOUPLE RWM_WIRE_COMPACT RWM_XPATH_REPAIR
)

# Emit `VAR=value` for every RWM_FORWARD knob that is SET in this process's
# environment. Word-splitting at the call site is intended:
#   ip netns exec "$NS" env $(rwm_forward_env) "$BIN" ...
# Values containing whitespace are not supported (no RWM_* knob takes one).
rwm_forward_env() {
    local v
    for v in "${RWM_FORWARD[@]}"; do
        if [[ -n "${!v+set}" ]]; then
            printf '%s=%s ' "$v" "${!v}"
        fi
    done
}

# ── HARNESS ERA: PER-LEG NETEM SEEDS (2026-08-19) ────────────────────────
#
# THE DEFECT THIS CLOSES, and the ERA BOUNDARY IT CREATES. Read the boundary
# first: it is the part that can silently corrupt a cross-era comparison.
#
# UNTIL 2026-08-19, `topo_dual.sh` took ONE `--seed` and handed the SAME value
# to both legs' `netem` qdiscs. netem's prng is seeded per qdisc, so at a
# SYMMETRIC cell (identical scenario on both legs — c7 = c2/c2, and every
# symmetric cell in this tree) the two paths' Gilbert-Elliott loss chains and
# delay-jitter draws were THE SAME REALIZATION INDEXED BY PACKET. The
# cross-path loss correlation was therefore rho = +1 BY CONSTRUCTION, not by
# measurement, at exactly the cells where pooling wins. This was read straight
# off the committed `-q.txt` qdisc captures (goal-gate "Eppen's Condition at
# c8" §2, THE SEED AUDIT: c7 3/3 captures same seed AND same params) and is
# recorded as a defect against the harness, not worked around.
#
# **THE ERA-COMPARABILITY CONSEQUENCE, stated so no reading mixes the two.**
#   * Ledgers captured BEFORE this change at a SYMMETRIC cell carry the
#     rho_loss = +1 coupling. Any statistic whose value depends on the
#     cross-path loss process — cross-path correlation, pooling benefit,
#     repair sharing, the variance of any per-path series — is conditioned on
#     it.
#   * Ledgers captured AFTER carry INDEPENDENT loss realizations at the same
#     cell, because `leg_seed` gives each leg its own stride.
#   * ASYMMETRIC cells (c8 = c2/c3) are NOT affected in the same way: the legs
#     already ran different GE parameters, so the chains differed even from a
#     shared seed. Their rho_loss was unconstrained and UNMEASURED in both
#     eras, and it still is.
#   * So: **a symmetric-cell number from before this date and one from after
#     are NOT the same measurement, and neither is a control for the other.**
#     A comparison that spans the boundary must either re-run the old arm with
#     an EXPLICIT equal-seed list (below — the old behaviour is still exactly
#     reachable) or state the boundary in its own verdict.
#
# THE DIAL, which is the point. `--seed` accepts a comma list, one value per
# leg; a single value derives the rest. So rho_loss is now a HARNESS DIAL with
# both ends reachable and neither of them accidental:
#   --seed 42        -> 42, 1042, 2042, 3042   INDEPENDENT legs   (the default)
#   --seed 42,42     -> 42, 42                 the rho = +1 arm we ran
#                                              unknowingly for the whole
#                                              previous era, now DELIBERATE
# The stride is 1000 and the derivation is `base + 1000*leg_index`: plain
# arithmetic, deterministic, reproducible from the base seed alone, and
# recorded in the `-q.txt` capture per leg so the audit that found the defect
# re-runs unchanged against the new era and reads `same_seed: False`.
LEG_SEED_STRIDE=1000

# `leg_seed <seed_spec> <leg_index>` -> the netem seed for that leg, or the
# empty string when no seed was requested (netem then draws its own, which is
# what the reverse/ACK direction has always done).
#
# `seed_spec` is a comma-separated list, and there are exactly TWO legal
# shapes — no third, partially-derived one:
#
#   ONE element   the BASE. Every leg is derived: `base + LEG_SEED_STRIDE*i`.
#   N elements    all N legs pinned explicitly, element `i` used verbatim.
#
# A spec with more than one element but FEWER than the topology's legs is a
# HARD ERROR, deliberately. The tempting rule — "use the listed ones, derive
# the rest" — would make `--seed 42,42` at a QUAD mean legs (42, 42, 2042,
# 3042): half the cell coupled at rho = +1 and half independent, which is
# neither arm and would be discovered only by reading the qdisc capture. That
# is the same class of silent mixed attribution the era note above exists to
# prevent, so it aborts instead.
leg_seed() {
    local spec="${1:-}" idx="${2:-0}"
    [[ -z "$spec" ]] && { echo ""; return 0; }
    local -a parts
    IFS=',' read -r -a parts <<< "$spec"
    if [[ "${#parts[@]}" -eq 1 ]]; then
        echo $(( ${parts[0]} + LEG_SEED_STRIDE * idx ))
    elif [[ "$idx" -lt "${#parts[@]}" ]]; then
        echo "${parts[$idx]}"
    else
        echo "REFUSED: --seed '$spec' lists ${#parts[@]} seeds but leg $idx \
was requested. Give ONE seed (all legs derived, stride $LEG_SEED_STRIDE) or \
one per leg (all pinned). A short list would silently mix coupled and \
independent legs in the same cell." >&2
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
