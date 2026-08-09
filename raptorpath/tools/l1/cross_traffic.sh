#!/bin/bash
# Cross-traffic L1 cell (roadmap item 6, feat/copa-compete): a raptorpath
# bulk transfer vs a COMPETING TCP Cubic flow sharing ONE shaped bottleneck
# (pathA of the dual-ns topology). The FIRST shared-bottleneck battery —
# it measures the named deployment gap that gates every substrate-CC
# default flip (goal-gate "Copa-Sole" caveat; Copa §2.2 competitive mode).
#
#   sudo env SEED=42 bash cross_traffic.sh <scenario> <arm> [bytes] [cubic_dur_s]
#
#   arm: solo    = Copa-sole + RWM_COPA_COMPETE=1, NO competitor
#                  (false-positive control: competitive mode must NOT engage
#                  and throughput must match the known solo numbers)
#        copa    = Copa-sole, compete OFF, vs 1 Cubic flow (starvation baseline)
#        compete = Copa-sole + RWM_COPA_COMPETE=1 vs 1 Cubic flow
#        bbr     = RWM_QUIC_CC=bbr vs the same Cubic flow (reference arm)
#
# All arms PLAIN (RWM_GEN=0), single path (pathA), RWM_DIAG=1. The Cubic
# competitor is iperf3 run INSIDE the rp-* namespaces (never on host
# interfaces), client in rp-cli -> server in rp-srv, so it shares pathA's
# netem bottleneck (cli0 egress qdisc) with the raptorpath flow.
#
# Emits one XTRESULT json line per invocation: both flows' Mbit/s over the
# raptorpath transfer window, throughput shares, Jain fairness index, the
# rp queue profile (wireQ/appQ p50/p90 from the sender DIAG) and the
# competitive-mode liveness counters (cmp= DIAG field).
set -uo pipefail
cd "$(dirname "$0")"
source ./lib.sh
# lib.sh forces `set -e` (MEASUREMENT DISCIPLINE #7 — the #61 battery lost
# whole arms to it silently): this script handles every failure explicitly
# (guards exit with distinct codes), so undo -e.
set +e
BIN="/home/vibe/raptorpath/target/release/raptorpath"

SCEN="${1:?scenario}"; ARM="${2:?arm (solo|copa|compete|bbr)}"
BYTES="${3:-25000000}"; DUR="${4:-150}"
IPERF_PORT=5209
SEED="${SEED:-42}"

# Arm env (PLAIN, single path). RWM_DIAG=1 always: the queue/compete
# profile IS a primary metric of this battery.
BASEENV="$(rwm_forward_env) RWM_GEN=0 RWM_DIAG=1"   # gate forwarding: ONE shared list in lib.sh
CUBIC=1
case "$ARM" in
    solo)    AENV="$BASEENV RWM_QUIC_CC=passthrough RWM_COPA_COMPETE=1"; CUBIC=0 ;;
    copa)    AENV="$BASEENV RWM_QUIC_CC=passthrough" ;;
    compete) AENV="$BASEENV RWM_QUIC_CC=passthrough RWM_COPA_COMPETE=1" ;;
    bbr)     AENV="$BASEENV RWM_QUIC_CC=bbr" ;;
    *) echo "unknown arm: $ARM" >&2; exit 2 ;;
esac
# (The former 9-knob passthrough loop is subsumed by rwm_forward_env in
# BASEENV above -- goal-gate "Gate-Forwarding Audit", 2026-08-09.)

cleanup() {
    pkill -x raptorpath 2>/dev/null || true
    pkill -f "iperf3 .*-p $IPERF_PORT" 2>/dev/null || true
    bash ./topo_dual.sh down >/dev/null 2>&1 || true
}
trap cleanup EXIT
cleanup

if pgrep -x raptorpath >/dev/null 2>&1; then
    echo "BUSY: raptorpath already running -- aborting" >&2
    exit 3
fi

# Topology up, with the seed-7 topo-ping abort protocol (discipline #8):
# GE loss can eat both verification echoes — retry ONCE, drop the
# invocation loudly (exit 5) on a double abort so the driver records it.
if ! bash ./topo_dual.sh up "$SCEN" "$SCEN" --seed "$SEED" >/dev/null 2>&1; then
    echo "TOPO-PING abort (seed=$SEED) — retrying once"
    if ! bash ./topo_dual.sh up "$SCEN" "$SCEN" --seed "$SEED" >/dev/null 2>&1; then
        echo "TOPO-PING double abort — invocation dropped (recorded)" >&2
        exit 5
    fi
fi

# MEASUREMENT DISCIPLINE: full env + binary hash echoed into the record.
echo "--- XT cell scen=$SCEN arm=$ARM seed=$SEED bytes=$BYTES cubic=$CUBIC dur=$DUR start=$(date +%T)"
echo "    env: SEED=$SEED $AENV"
echo "    bin: $(sha256sum "$BIN" | cut -c1-16)  $(uname -r)"

# raptorpath server (single path, pathA only)
# shellcheck disable=SC2086
ip netns exec "$NS_SRV" env $AENV "$BIN" perf --server --bind "10.77.0.2:7000" \
    --window-reliable --protocol-hint bulk >/tmp/xt-s.log 2>&1 &
for _ in $(seq 1 20); do
    ip netns exec "$NS_SRV" ss -uln 2>/dev/null | grep -q ':7000' && break
    sleep 0.3
done

# Cubic competitor: iperf3 server + client INSIDE the rp namespaces.
rm -f /tmp/xt-iperf.json
if [[ "$CUBIC" == "1" ]]; then
    ip netns exec "$NS_SRV" iperf3 -s -D -p "$IPERF_PORT" --pidfile /tmp/xt-iperf-srv.pid
    sleep 0.3
    ip netns exec "$NS_CLI" iperf3 -c 10.77.0.2 -p "$IPERF_PORT" -C cubic \
        -t "$DUR" --json >/tmp/xt-iperf.json &
    IPERF_CLI_PID=$!
    # Head start: the rp flow joins an ESTABLISHED buffer-filling Cubic.
    sleep 2
fi
sleep 1

T0=$(date +%s.%N)
# shellcheck disable=SC2086
timeout 700 ip netns exec "$NS_CLI" env $AENV "$BIN" perf --client \
    --peer "10.77.0.2:7000" --bind "10.77.0.1:0" \
    --window-reliable --protocol-hint bulk \
    --bytes "$BYTES" --runs 1 2>&1 | tee /tmp/xt-c.log \
    | grep -E "summary|warmup|dnf" | tail -4 \
    || echo "{\"dnf\":true,\"arm\":\"$ARM\"}"
T1=$(date +%s.%N)

# Stop the competitor: SIGINT makes iperf3 emit its JSON with the intervals
# collected so far (the per-interval series is what the overlap math reads).
if [[ "$CUBIC" == "1" ]]; then
    if kill -0 "$IPERF_CLI_PID" 2>/dev/null; then
        kill -INT "$IPERF_CLI_PID" 2>/dev/null || true
    fi
    wait "$IPERF_CLI_PID" 2>/dev/null || true
    pkill -f "iperf3 -s -D -p $IPERF_PORT" 2>/dev/null || true
fi

# --- MECHANISM-LIVENESS GUARDS (MEASUREMENT DISCIPLINE #1/#6) ---------------
# ANSI-strip first: tracing colorizes field names, so "compete=true" spans
# escape codes in the raw log.
CLOG=/tmp/xt-c-clean.log
sed 's/\x1b\[[0-9;]*m//g' /tmp/xt-c.log > "$CLOG"
if [[ "$ARM" != "bbr" ]]; then
    if ! grep -q "feed ACTIVE" "$CLOG"; then
        echo "FATAL: passthrough arm without 'feed ACTIVE' echo — Copa-sole not live; numbers INVALID" >&2
        exit 7
    fi
fi
if [[ "$ARM" == "compete" || "$ARM" == "solo" ]]; then
    if ! grep -q "compete=true" "$CLOG"; then
        echo "FATAL: RWM_COPA_COMPETE requested but compete=true echo missing — switching not live" >&2
        exit 7
    fi
fi
if [[ "$CUBIC" == "1" ]] && ! [[ -s /tmp/xt-iperf.json ]]; then
    echo "FATAL: competitor arm but no iperf3 JSON — Cubic flow never ran; cell INVALID" >&2
    exit 7
fi

# --- Metrics: shares over the rp transfer window + Jain + queue profile -----
python3 - "$SCEN" "$ARM" "$SEED" "$BYTES" "$T0" "$T1" "$CUBIC" <<'EOF'
import json, re, sys
scen, arm, seed, nbytes, t0, t1, cubic = sys.argv[1:8]
t0, t1 = float(t0), float(t1)

rp_mbps = rp_secs = None
dnf = False
for line in open("/tmp/xt-c-clean.log", errors="replace"):
    line = line.strip()
    if line.startswith("{"):
        try: j = json.loads(line)
        except Exception: continue
        if j.get("run") == 1:
            if j.get("dnf"): dnf = True
            else: rp_mbps, rp_secs = j["mbps"], j["seconds"]

# rp transfer window: the timed object ends ~at client exit; the run line's
# `seconds` spans the object. Subtract the summary/teardown slack from T1.
win_end = t1 - 0.2
win_start = win_end - (rp_secs if rp_secs else (t1 - t0))

cubic_mbps = 0.0
if cubic == "1":
    try:
        ij = json.load(open("/tmp/xt-iperf.json"))
        base = ij["start"]["timestamp"]["timesecs"]
        acc_bits = acc_secs = 0.0
        for iv in ij.get("intervals", []):
            s = base + iv["sum"]["start"]; e = base + iv["sum"]["end"]
            lo, hi = max(s, win_start), min(e, win_end)
            if hi > lo and e > s:
                frac = (hi - lo) / (e - s)
                acc_bits += iv["sum"]["bits_per_second"] * (e - s) * frac
                acc_secs += (hi - lo)
        if acc_secs > 0: cubic_mbps = acc_bits / acc_secs / 1e6
    except Exception as ex:
        print(f"WARN: iperf parse failed: {ex}", file=sys.stderr)

# DIAG scrape: wireQ/appQ (wrtt/rtt vs rtp) + competitive-mode counters.
wq, aq = [], []
cmp_sw, cmp_c, cmp_n, dmin = 0, 0, 0, None
pat = re.compile(r"rtt=(\d+)/wrtt=(\d+)/rtp(\d+)ms")
cpat = re.compile(r"cmp=([CD])(\d+)/([0-9.]+)")
nline = 0
for line in open("/tmp/xt-c-clean.log", errors="replace"):
    m = pat.search(line)
    if m:
        nline += 1
        if nline >= 4:  # steady state (pooling rule of the prior batteries)
            rtt, wrtt, rtp = map(int, m.groups())
            if wrtt > 0 and rtp > 0:
                wq.append(max(wrtt - rtp, 0)); aq.append(max(rtt - rtp, 0))
    c = cpat.search(line)
    if c:
        cmp_n += 1
        cmp_sw = max(cmp_sw, int(c.group(2)))
        if c.group(1) == "C": cmp_c += 1
        d = float(c.group(3))
        dmin = d if dmin is None else min(dmin, d)

def q(v, p):
    if not v: return None
    v = sorted(v); i = min(len(v) - 1, int(round(p * (len(v) - 1))))
    return v[i]

tot = (rp_mbps or 0.0) + cubic_mbps
share = (rp_mbps or 0.0) / tot if tot > 0 else None
jain = None
if cubic == "1" and rp_mbps is not None and tot > 0:
    jain = tot * tot / (2.0 * ((rp_mbps) ** 2 + cubic_mbps ** 2))

print("XTRESULT " + json.dumps({
    "scen": scen, "arm": arm, "seed": int(seed), "bytes": int(nbytes),
    "dnf": dnf, "rp_mbps": rp_mbps, "rp_seconds": rp_secs,
    "cubic_mbps": round(cubic_mbps, 3) if cubic == "1" else None,
    "rp_share": round(share, 4) if share is not None else None,
    "jain": round(jain, 4) if jain is not None else None,
    "wq_p50": q(wq, 0.5), "wq_p90": q(wq, 0.9),
    "aq_p50": q(aq, 0.5), "aq_p90": q(aq, 0.9),
    "cmp_lines": cmp_n, "cmp_switches": cmp_sw,
    "cmp_c_frac": round(cmp_c / cmp_n, 3) if cmp_n else None,
    "cmp_delta_min": dmin,
}))
EOF
echo "    done $(date +%T)"
