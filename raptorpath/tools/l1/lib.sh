#!/bin/bash
# Shared helpers for the L1 harness. See docs/l1-harness-plan.md.
#
# SAFETY: this file encodes the hard rules that protect SSH access to the
# test VM. All shaping happens on veth devices inside rp-* namespaces.

set -euo pipefail

# The VM's management interface — carries our SSH session. Never touched.
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

# ── Gate forwarding ──────────────────────────────────────────────────────
#
# The one list of `RWM_*` knobs the harness forwards to the binary, and the
# one function that turns it into an `env` prefix. Every driver that launches
# the binary sources this file and passes `$(rwm_forward_env)`.
#
# `sudo env VAR=… → bash driver → ip netns exec ns env $TENV` also delivers a
# var by plain process-environment inheritance whether or not a list names it;
# the list makes the forwarding total and explicit so every set gate is
# visible on the command line (docs/measurement-discipline.md rule 15).
#
# Enforcement: `raptorpath`'s `gate_forwarding_list_covers_the_engine_surface`
# test parses this array and fails if any `RWM_*` the engine reads is missing.
# Adding a gate to the engine without adding it here fails the suite.
RWM_FORWARD=(
    RWM_ACKDIAG RWM_ACKDIAG_WINDOW_US
    RWM_RTT_DUMP RWM_RTT_DUMP_MAX
    RWM_SUCC_DUMP RWM_SUCC_DUMP_MAX
    RWM_ACK_MERGE RWM_ANCHOR_HYGIENE RWM_ASTAR_ANCHOR RWM_CC_PACE
    RWM_CC_PACE_HR RWM_CHARGE_RECOVERY RWM_CLOCK_GAP RWM_CODED_SRC RWM_COLD_PLACE RWM_COPA_COMPETE
    RWM_COMPOSED_CAP RWM_COPA_DELTA RWM_COPA_FEED RWM_COPA_WIRE RWM_CPUPROF
    RWM_DERIVED_SWEEP RWM_DIAG
    RWM_EMIT_BATCH RWM_EMIT_BURST RWM_EST_CADENCE RWM_FDIAG
    RWM_GEN RWM_GEN_INFLIGHT RWM_GEN_PIPE
    RWM_GEN_R RWM_GEN_RATE RWM_GEN_RATE_FLOOR RWM_HONEST_ANCHOR
    RWM_HONEST_CAP RWM_HONEST_K
    RWM_INFL_BDP RWM_INFL_CAP RWM_L0_NETEM RWM_L0_SEED
    RWM_LATE_BRAKE RWM_LOSS_SENT_TRUTH
    RWM_MIN_R RWM_MSTAR_ANCHOR RWM_MTU_FLOOR RWM_NO_REACTIVE
    RWM_OOO_RETAIN RWM_PERF_TIMEOUT_S
    RWM_PFRAC RWM_PIPELINE RWM_PLACE_T
    RWM_PLACE_T_DERIVED RWM_PLACE_HOL RWM_PLACE_WDIV_DERIVED
    RWM_PLAIN_RS RWM_POOL_ANCHOR RWM_PROACTIVE_PACER
    RWM_QUIC_CC RWM_RDIAG RWM_REACT_CAP RWM_REASM_BDP
    RWM_RECOV_MP RWM_RECOV_MP_LAW RWM_RECOV_MP_LIVE RWM_RECOV_SP
    RWM_RELEASE_1TO1
    RWM_REPAIR_WAIT RWM_REPORT_GENS RWM_RSTAR_TAIL RWM_RS_ATTR
    RWM_RS_TRACE RWM_SIDLE_DERIVED RWM_STORE
    RWM_STORE_BOOT RWM_STORE_CAP_UNIFIED RWM_STORE_GAIN
    RWM_STORE_PATHS RWM_STORE_PATH_POOL RWM_STORE_SACK_RELEASE
    RWM_SUM_CAP RWM_DELTA_CAP
    RWM_HOLDDOWN_Q
    RWM_DELTA RWM_COMPLETION_EXPOSURE
    RWM_REFRESH_FLOOR_US
    RWM_RECV_REQUEST_LAW RWM_RANK_FEEDBACK
    RWM_TAPER_R RWM_THREE_TERM RWM_TRACE RWM_UNIFIED RWM_UNIFIED_SHED
    RWM_WALLDIAG RWM_WINDOW RWM_WIRE_COMPACT RWM_XPATH_REPAIR
)

# Emit `VAR=value` for every RWM_FORWARD knob that is set in this process's
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

# ── Per-leg netem seeds ──────────────────────────────────────────────────
#
# netem's prng is seeded per qdisc. Handing the same seed to both legs of a
# symmetric cell (identical scenario on both legs, e.g. c7 = c2/c2) makes the
# two paths' Gilbert-Elliott loss chains and delay-jitter draws the same
# realization indexed by packet: cross-path loss correlation rho = +1 by
# construction, at exactly the cells where pooling wins. So each leg gets its
# own seed by default.
#
# Comparability: a symmetric-cell measurement taken with equal leg seeds and
# one taken with per-leg seeds are not the same measurement, and neither is a
# control for the other. Any statistic that depends on the cross-path loss
# process — cross-path correlation, pooling benefit, repair sharing, the
# variance of any per-path series — differs between them. Asymmetric cells
# (c8 = c2/c3) run different GE parameters per leg, so their chains differ
# even from a shared seed; their rho_loss is unconstrained and unmeasured.
#
# The dial: `--seed` accepts a comma list, one value per leg; a single value
# derives the rest. So rho_loss is a harness dial with both ends reachable:
#   --seed 42        -> 42, 1042, 2042, 3042   independent legs   (the default)
#   --seed 42,42     -> 42, 42                 the rho = +1 arm
# The stride is 1000 and the derivation is `base + 1000*leg_index`: plain
# arithmetic, deterministic, reproducible from the base seed alone, and
# recorded in the `-q.txt` capture per leg.
LEG_SEED_STRIDE=1000

# `leg_seed <seed_spec> <leg_index>` -> the netem seed for that leg, or the
# empty string when no seed was requested (netem then draws its own, which is
# what the reverse/ACK direction has always done).
#
# `seed_spec` is a comma-separated list, and there are exactly two legal
# shapes — no third, partially-derived one:
#
#   one element   the base. Every leg is derived: `base + LEG_SEED_STRIDE*i`.
#   N elements    all N legs pinned explicitly, element `i` used verbatim.
#
# A spec with more than one element but fewer than the topology's legs is a
# hard error. "Use the listed ones, derive the rest" would make `--seed 42,42`
# at a quad mean legs (42, 42, 2042, 3042): half the cell coupled at
# rho = +1 and half independent, which is neither arm and would be
# discovered only by reading the qdisc capture.
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

# Scenario table — identical parameterization to ADR-0051 / paper §2.3.
# Fields: rate one_way_ms jitter_ms ge_p ge_q
scenario_params() {
    case "$1" in
        c1|dc)       echo "1gbit   1   0  0.05 50" ;;
        c2|wifi)     echo "100mbit 5   3  1.3  50" ;;
        c3|lte)      echo "20mbit  20  5  2    40" ;;
        # `c3hg` -- "c3, heavy, GE form": the reachability cell for the
        # completion glide. The glide's fully-exposed target is
        # `BULK_TAIL_BUDGET = 0.05`, so on any channel cleaner than 5 % the
        # r corner survives full exposure and r* = 0 whatever chi does
        # (paper §4.5; tests/chi_reachability.rs). `c3`'s eps = 2/(2+40) =
        # 4.762 % sits just below that line.
        #
        # c3's rate/one-way/jitter and burst structure q = 40 exactly; `p` is
        # the only changed field, solved from eps = p/(p+q) at 5.8 %:
        # p = 0.058*40/(1-0.058) = 2.46284...  ->  eps = 5.8000 %.
        # sigma^2_burst = 1 + 2(1-p-q)/(p+q) = 3.7101 (c3's is 3.762).
        #
        # Not named `c3heavy`: that is an L0 simulator scenario
        # (src/transport/l0_netem.rs) whose loss law is a Weibull heavy tail
        # (k = 0.5, theta = 0.55, E[burst] = 6.2) that `tc netem gemodel`
        # cannot represent. `c3hg` matches c3heavy's loss rate, not its burst
        # law.
        c3hg)        echo "20mbit  20  5  2.4629 40" ;;
        c4|sat)      echo "20mbit  100 10 3    30" ;;
        c5|badwifi)  echo "50mbit  5   3  5.3  30" ;;
        clean)       echo "100mbit 5   0  0    100" ;;
        # FEC-vs-ARQ crossover RTT sweep: c2 loss/bw
        # (100mbit, GE 1.3/50 ≈ 2.5% mean loss) with jitter=0 so RTT is the only
        # swept variable. one_way = RTT/2.  RTT ∈ {10,30,50,100,200} ms.
        c2r10)       echo "100mbit 5   0  1.3  50" ;;
        c2r30)       echo "100mbit 15  0  1.3  50" ;;
        c2r50)       echo "100mbit 25  0  1.3  50" ;;
        c2r100)      echo "100mbit 50  0  1.3  50" ;;
        c2r200)      echo "100mbit 100 0  1.3  50" ;;
        # Receiver-tail + FEC-favorable-regime sweep: the
        # same c2 pipe (100mbit, jitter=0) at RTT{100,200} but with HIGHER GE
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

# Stop the engine the way its receiver can see (the exit flush).
# The engine handles SIGINT and SIGTERM as one shutdown trigger; the receiver's
# diagnostic block ([SUCC]/[ETA]/[LAT]/[LATE]/[REQ]/[RANK], `final=1`) flushes
# on that path and on Drop, and never on SIGKILL. A bare `pkill -x raptorpath`
# is a TERM with no grace: the topology teardown that follows deletes the
# namespaces underneath a process that is still writing its last lines. So:
# TERM, a bounded wait (3 s) for the exit, and only then KILL as the last
# resort. `-x raptorpath` and nothing else (docs/measurement-discipline.md,
# "The VM protocol").
stop_raptorpath() {
    pkill -TERM -x raptorpath 2>/dev/null || true
    local _i
    for _i in $(seq 1 30); do
        pgrep -x raptorpath >/dev/null 2>&1 || return 0
        sleep 0.1
    done
    pkill -KILL -x raptorpath 2>/dev/null || true
}
