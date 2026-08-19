#!/bin/bash
# LOCAL GATE for THE PER-LEG DELIVERED-LATENCY PROBE — `latt_probe.py` and the
# `RWM_LATPROBE` block of `perf_rwm_c.sh`. No root, no namespaces, no VM, no
# kernel: the probe's failure modes are all statements about WHICH COMMANDS ARE
# ISSUED and WHAT ARITHMETIC IS DONE ON THE OUTPUT, and both are checkable
# against stubs and synthetic `ping` text.
#
#   usage: bash test_latprobe.sh     (exit 0 = pass; one line per check)
#
# WHY THIS EXISTS, and it is not hypothetical. The era battery scored a
# delivered-latency claim on a probe that (a) sampled ONE leg of a two-leg
# asymmetric cell, (b) was SIGTERM'd so `ping` never wrote its
# transmitted/received summary and the loss columns were None on all 204
# invocations, and (c) computed tail percentiles over the SURVIVING probes of a
# deliberately lossy link — which censors exactly the worst samples and biases
# the tail LOW. Not one of those three was catchable by any existing test,
# because nothing read the probe's command stream and nothing exercised its
# arithmetic on known input. This file does both.
#
# MEASUREMENT DISCIPLINE 1 (prove the mechanism under test executes) is why the
# command-stream half exists at all: an assertion that the parser computes a
# censoring fraction is worth nothing if the harness never routes a second leg's
# file into it.
set -uo pipefail
cd "$(dirname "$0")"

STUB=$(mktemp -d)
LOG="$STUB/cmds.log"
trap 'rm -rf "$STUB"' EXIT

FAIL=0
ok()   { printf 'ok    %s\n' "$1"; }
bad()  { printf 'FAIL  %s\n' "$1"; FAIL=1; }
ckeq() { # desc want got
    if [[ "$2" == "$3" ]]; then ok "$(printf '%-62s (%s)' "$1" "$3")"
    else bad "$(printf '%-62s want %s got %s' "$1" "$2" "$3")"; fi
}

# ── PART 1: THE ARITHMETIC, on synthetic `ping` output ────────────────────
# Every fixture below is written by hand so the expected answer is known
# independently of the code under test. `-D` timestamps are included because
# that is the format the harness actually produces.

mkping() { # file  n_replies  first_seq_gap_list  summary_tx  summary_rx  rtt_base
    local f="$1" n="$2" tx="$3" rx="$4"
    : > "$f"
    echo "PING 10.77.0.2 (10.77.0.2) 56(84) bytes of data." >> "$f"
    local i
    for ((i = 1; i <= n; i++)); do
        printf '[17556000%02d.123456] 64 bytes from 10.77.0.2: icmp_seq=%d ttl=64 time=%d.0 ms\n' \
            "$i" "$i" "$i" >> "$f"
    done
    if [[ -n "$tx" ]]; then
        echo "" >> "$f"
        echo "--- 10.77.0.2 ping statistics ---" >> "$f"
        echo "$tx packets transmitted, $rx received, 0% packet loss, time 5000ms" >> "$f"
    fi
}

# (a) NO LOSS: 100 replies, summary says 100/100 -> censor 0, everything scoreable.
mkping "$STUB/p-clean.txt" 100 100 100
J=$(python3 ./latt_probe.py --json "$STUB/p-clean.txt")
ckeq "clean leg: sent"            "100"   "$(echo "$J" | python3 -c 'import json,sys;print(json.load(sys.stdin)[0]["sent"])')"
ckeq "clean leg: recv"            "100"   "$(echo "$J" | python3 -c 'import json,sys;print(json.load(sys.stdin)[0]["recv"])')"
ckeq "clean leg: censor_frac"     "0.0"   "$(echo "$J" | python3 -c 'import json,sys;print(json.load(sys.stdin)[0]["censor_frac"])')"
ckeq "clean leg: p99 scoreable"   "True"  "$(echo "$J" | python3 -c 'import json,sys;print(json.load(sys.stdin)[0]["p99_scoreable"])')"
ckeq "clean leg: leg_unscoreable" "False" "$(echo "$J" | python3 -c 'import json,sys;print(json.load(sys.stdin)[0]["leg_unscoreable"])')"
# Nearest-rank on 1..100 ms: p50 -> index int(0.5*100)=50 -> the 51st value.
ckeq "clean leg: p50 = nearest-rank, era-battery estimator" "51.0" \
    "$(echo "$J" | python3 -c 'import json,sys;print(json.load(sys.stdin)[0]["p50"])')"

# (b) 5 % CENSORING — the c8 leg-B GE floor (2/42 = 4.76 %), rounded up. 95 of
#     100 probes came back. THE STRUCTURAL RULE MUST KILL p99 AND SPARE p50.
#     This is the case that decides whether the era battery's `ping_p99` column
#     ever meant anything: 0.99 > 1 - 0.05, so it did not.
mkping "$STUB/p-ge.txt" 95 100 95
J=$(python3 ./latt_probe.py --json "$STUB/p-ge.txt")
ckeq "5% censored: censor_frac"        "0.05"  "$(echo "$J" | python3 -c 'import json,sys;print(json.load(sys.stdin)[0]["censor_frac"])')"
ckeq "5% censored: p99 INSIDE the censored tail" "True" \
    "$(echo "$J" | python3 -c 'import json,sys;print(json.load(sys.stdin)[0]["p99_censored"])')"
ckeq "5% censored: p99 NOT scoreable"  "False" "$(echo "$J" | python3 -c 'import json,sys;print(json.load(sys.stdin)[0]["p99_scoreable"])')"
ckeq "5% censored: p95 still scoreable (0.95 < 0.95x)" "True" \
    "$(echo "$J" | python3 -c 'import json,sys;print(json.load(sys.stdin)[0]["p95_scoreable"])')"
ckeq "5% censored: p50 still scoreable" "True" \
    "$(echo "$J" | python3 -c 'import json,sys;print(json.load(sys.stdin)[0]["p50_scoreable"])')"

# (c) THE CONTRACT BAR: 25 % censoring kills the WHOLE leg, p50 included.
mkping "$STUB/p-bad.txt" 75 100 75
J=$(python3 ./latt_probe.py --json "$STUB/p-bad.txt")
ckeq "25% censored: censor_frac"       "0.25"  "$(echo "$J" | python3 -c 'import json,sys;print(json.load(sys.stdin)[0]["censor_frac"])')"
ckeq "25% censored: leg_unscoreable"   "True"  "$(echo "$J" | python3 -c 'import json,sys;print(json.load(sys.stdin)[0]["leg_unscoreable"])')"
ckeq "25% censored: p50 NOT scoreable" "False" "$(echo "$J" | python3 -c 'import json,sys;print(json.load(sys.stdin)[0]["p50_scoreable"])')"
if python3 ./latt_probe.py "$STUB/p-bad.txt" | grep -q 'UNSCOREABLE(contract'; then
    ok "25% censored: the printed line SAYS UNSCOREABLE beside the percentile"
else
    bad "25% censored: the printed line does not carry the contract verdict"
fi

# (d) THE SIGTERM CASE — no summary at all, which is exactly what the era
#     battery produced. `sent` must fall back to max(icmp_seq) and SAY SO, and
#     the fallback must never be reported as a clean count.
mkping "$STUB/p-nosum.txt" 40 "" ""
J=$(python3 ./latt_probe.py --json "$STUB/p-nosum.txt")
ckeq "no summary: sent_source flagged as a lower bound" "max_icmp_seq(LOWER BOUND)" \
    "$(echo "$J" | python3 -c 'import json,sys;print(json.load(sys.stdin)[0]["sent_source"])')"
ckeq "no summary: summary_tx is None (the era battery's silent column)" "None" \
    "$(echo "$J" | python3 -c 'import json,sys;print(json.load(sys.stdin)[0]["summary_tx"])')"
if python3 ./latt_probe.py "$STUB/p-nosum.txt" | grep -q 'sent_source=max_icmp_seq'; then
    ok "no summary: the printed line discloses the fallback denominator"
else
    bad "no summary: the fallback denominator is not disclosed on the line"
fi

# (e) A GAP IN THE SEQUENCE — 90 replies but seq runs to 100, no summary. The
#     lower-bound denominator must still find the censoring rather than read 0.
: > "$STUB/p-gap.txt"
for i in $(seq 1 100); do
    [ $((i % 10)) -eq 0 ] && continue
    printf '[1755600000.1234] 64 bytes from 10.78.0.2: icmp_seq=%d ttl=64 time=%d.5 ms\n' "$i" "$i" >> "$STUB/p-gap.txt"
done
J=$(python3 ./latt_probe.py --json "$STUB/p-gap.txt")
ckeq "seq gaps, no summary: recv" "90"  "$(echo "$J" | python3 -c 'import json,sys;print(json.load(sys.stdin)[0]["recv"])')"
# 100 probes were sent; seq 10,20,...,100 were lost. The last one, seq 100, is
# UNRECOVERABLE from the replies — the highest surviving seq is 99. So the
# fallback denominator reads 99, not 100, and the censoring comes out 9/99 =
# 9.09 % against a true 10 %. THAT UNDERSTATEMENT IS THE POINT, and it is
# asserted here rather than papered over: it is precisely why `sent_source`
# labels this denominator a LOWER BOUND, and why the reaper goes to the trouble
# of making `ping` write its own summary instead of settling for this.
ckeq "seq gaps, no summary: sent floor is max surviving seq (99, NOT 100)" "99" \
    "$(echo "$J" | python3 -c 'import json,sys;print(json.load(sys.stdin)[0]["sent"])')"
ckeq "seq gaps, no summary: censoring FOUND but UNDERSTATED (9/99 vs true 10%)" "0.0909" \
    "$(echo "$J" | python3 -c 'import json,sys;print(json.load(sys.stdin)[0]["censor_frac"])')"

# (f) THE EMPTY PROBE — the leg that produced nothing. It must be an explicit
#     NO-PROBE-DATA line, never a silent zero-length sample that a scorer could
#     average into a verdict.
: > "$STUB/p-empty.txt"
if python3 ./latt_probe.py "$STUB/p-empty.txt" | grep -q 'NO-PROBE-DATA'; then
    ok "empty probe file: reported as NO-PROBE-DATA"
else
    bad "empty probe file: not reported"
fi

# (g) TWO LEGS, ONE INVOCATION — the shape the duals need, and the ordering
#     must be stable so `leg=0` is always path A.
OUT2=$(python3 ./latt_probe.py "$STUB/p-clean.txt" "$STUB/p-ge.txt")
ckeq "two legs: two output lines" "2" "$(echo "$OUT2" | grep -c 'LATPROBE-LEG')"
ckeq "two legs: leg indices are 0 then 1" "leg=0 leg=1" \
    "$(echo "$OUT2" | grep -o 'leg=[01]' | tr '\n' ' ' | sed 's/ $//')"

# ── PART 2: THE COMMAND STREAM — does the harness actually probe every leg? ──
# `perf_rwm_c.sh` is not runnable end to end without a kernel, so the probe
# block is extracted and executed against an `ip` stub. Extraction is BY
# MARKER, from the file under test, so this gate cannot pass against a copy
# that has drifted from the shipped script.
mkdir -p "$STUB/bin"
cat > "$STUB/bin/ip" <<'STUBEOF'
#!/bin/bash
echo "ip $*" >> "$STUB_LOG"
# A BOUNDED `ping` emulator: 12 replies with ONE drop (seq 7), then the
# statistics summary `iputils` writes from its `sigexit` handler, then exit.
#
# WHY BOUNDED RATHER THAN SIGNAL-DRIVEN, stated because it is a real limit on
# what this half of the gate proves. A shell sets SIGINT to SIG_IGN for jobs
# started with `&` when job control is off, and POSIX then forbids a child
# SHELL from trapping it — so a bash stub CANNOT emulate the signal path at
# all, no matter how it is written. Real `ping` is a C program using
# `sigaction`, which is not subject to that rule; the reaper's INT -> ALRM ->
# TERM escalation is written for exactly that uncertainty. So: the SIGNAL
# CHOICE is asserted textually against the extracted block above, and this
# stub asserts the parts a stub honestly can — leg count, addresses, files,
# and the censoring arithmetic flowing through the reaper's own output.
peer=""; for a in "$@"; do peer="$a"; done
for n in 1 2 3 4 5 6 8 9 10 11 12; do
    echo "[1755600000.0000] 64 bytes from $peer: icmp_seq=$n ttl=64 time=$n.0 ms"
done
echo ""
echo "--- $peer ping statistics ---"
echo "12 packets transmitted, 11 received, 8% packet loss, time 600ms"
exit 0
STUBEOF
chmod +x "$STUB/bin/ip"
export STUB_LOG="$LOG"
: > "$LOG"

# The block under test, lifted verbatim between its own comment markers.
awk '/^PING_PIDS=\(\)/,/^fi$/' ./perf_rwm_c.sh > "$STUB/probe_up.sh"
awk '/^if \[\[ "\$\{#PING_PIDS\[@\]\}" -gt 0 \]\]; then/,/^fi$/' ./perf_rwm_c.sh > "$STUB/probe_down.sh"
[ -s "$STUB/probe_up.sh" ]   || bad "could not extract the probe LAUNCH block from perf_rwm_c.sh"
[ -s "$STUB/probe_down.sh" ] || bad "could not extract the probe REAP block from perf_rwm_c.sh"
ckeq "reap block uses SIGINT, not the default SIGTERM" "1" \
    "$(grep -c 'kill -INT' "$STUB/probe_down.sh")"
ckeq "reap block's pkill pattern is leg-general, not hard-coded to 10.77" "0" \
    "$(grep -c 'pkill -f "ping -i 0.05 -W 2 -D 10.77.0.2"' "$STUB/probe_down.sh")"
ckeq "launch block does not hard-code path A" "0" \
    "$(grep -c -- '-D 10\.77\.0\.2' "$STUB/probe_up.sh")"

run_probe() { # nlegs
    local n="$1"
    : > "$LOG"
    rm -f /tmp/rwm-lattest-ping-*.txt
    (
        export PATH="$STUB/bin:$PATH"
        NS_CLI=rp-cli
        RWM_LATPROBE=1
        CLI_LEGS=()
        for ((i = 0; i < n; i++)); do CLI_LEGS+=("cli$i"); done
        # shellcheck disable=SC1090
        source "$STUB/probe_up.sh"
        sleep 0.4
        # shellcheck disable=SC1090
        source "$STUB/probe_down.sh"
    ) > "$STUB/probe_out.txt" 2>&1
}

run_probe 2
ckeq "dual: TWO ping commands issued" "2" "$(grep -c 'netns exec rp-cli ping' "$LOG")"
ckeq "dual: leg A peer 10.77.0.2 probed" "1" "$(grep -c -- '-D 10.77.0.2' "$LOG")"
ckeq "dual: leg B peer 10.78.0.2 probed — THE LEG THE ERA BATTERY NEVER SAW" "1" \
    "$(grep -c -- '-D 10.78.0.2' "$LOG")"
for f in /tmp/rwm-ping-0.txt /tmp/rwm-ping-1.txt; do
    if [ -s "$f" ]; then ok "dual: $f exists and is non-empty"
    else bad "dual: $f missing or empty"; fi
done
ckeq "dual: the reap printed ONE LATPROBE-LEG line per leg" "2" \
    "$(grep -c 'LATPROBE-LEG' "$STUB/probe_out.txt")"
if grep -q 'censor=' "$STUB/probe_out.txt"; then
    ok "dual: the reap's own output carries the censoring fraction"
else
    bad "dual: the reap printed percentiles with no censoring fraction"
fi
# END TO END through the shipped reaper: the stub sent 12 and delivered 11, so
# the ledger line must read the SUMMARY's denominator (12) and not the reply
# count (11). A reaper that failed to make `ping` write its summary would show
# sent=12 sourced from `max_icmp_seq` and a censoring of 1/12 by luck — so the
# `sent_source` absence is asserted too.
ckeq "dual: censoring computed from ping's OWN summary on both legs" "2" \
    "$(grep -c 'sent=12 recv=11 censor=8.33%' "$STUB/probe_out.txt")"
ckeq "dual: no lower-bound fallback was needed" "0" \
    "$(grep -c 'sent_source=' "$STUB/probe_out.txt")"
if [ -s /tmp/rwm-ping.txt ]; then
    ok "dual: the legacy /tmp/rwm-ping.txt is still written (leg A)"
else
    bad "dual: the legacy /tmp/rwm-ping.txt was dropped — existing callers break"
fi

# QUAD-SAFETY, which is the `pid < 2` defect restated for the probe: the leg
# count is DERIVED, so four legs must produce four probes with no edit here.
run_probe 4
ckeq "quad: FOUR ping commands issued" "4" "$(grep -c 'netns exec rp-cli ping' "$LOG")"
ckeq "quad: the four peers are 10.77-10.80 .0.2" "4" \
    "$(grep -cE -- '-D 10\.(77|78|79|80)\.0\.2' "$LOG")"
ckeq "quad: FOUR per-leg files" "4" "$(ls /tmp/rwm-ping-[0-3].txt 2>/dev/null | wc -l | tr -d ' ')"

run_probe 1
ckeq "single: ONE ping command issued" "1" "$(grep -c 'netns exec rp-cli ping' "$LOG")"

# The probe must be OFF by default, or every existing driver silently gains an
# unmeasured competing flow.
(
    export PATH="$STUB/bin:$PATH"
    : > "$LOG"
    NS_CLI=rp-cli
    CLI_LEGS=(cli0 cli1)
    # shellcheck disable=SC1090
    source "$STUB/probe_up.sh"
) >/dev/null 2>&1
ckeq "RWM_LATPROBE unset: NO probe is launched" "0" "$(grep -c 'ping' "$LOG")"

rm -f /tmp/rwm-ping-[0-3].txt

echo
if [ "$FAIL" -eq 0 ]; then echo "test_latprobe.sh: ALL CHECKS PASS"; else echo "test_latprobe.sh: FAILURES ABOVE"; fi
exit "$FAIL"
