#!/bin/bash
# LOCAL GATE for topo_quad.sh AND topo_dual.sh — runs each topology script
# against STUB `ip`/`tc`/`sysctl`/`ping` binaries and asserts on the exact
# command stream it would have issued. No root, no namespaces, no VM, no
# kernel.
#
# WHY THIS EXISTS. A topology script's failure modes are (a) touching a device
# it must never touch, and (b) widening an array without widening the bound
# that indexes it — the `pid < 2` defect the SF bench taught, where
# `MAX_PATHS` went from 2 to 4 and three per-path gauge writes kept their
# hard-coded `< 2` guard, so two of the four legs read 0 NO MATTER WHAT and
# the assertion built on them could not fail. Both failure modes are
# statements about the COMMANDS ISSUED, and both are checkable without a
# kernel. Neither was checkable at all before this file: `topo_dual.sh` has
# never had a test of any kind, and its shared-seed defect survived a whole
# harness era precisely because nothing read its command stream.
#
#   usage: bash test_topo.sh        (exit 0 = pass; prints one line per check)

set -uo pipefail
cd "$(dirname "$0")"

STUB=$(mktemp -d)
LOG="$STUB/cmds.log"
trap 'rm -rf "$STUB"' EXIT

# The topology scripts now source `abort_witness.sh`, which writes a record to
# `$AW_FILE`. Point it inside the stub dir so this gate never touches the real
# `/tmp/rwm-abort.txt` a live battery may be mid-way through writing.
export AW_FILE="$STUB/abort.txt"
# The retry bound is read from the environment by `aw_ping`, so the cases below
# can shrink it without editing the file under test. The DEFAULT is asserted
# separately, against `abort_witness.sh` itself, so a silent re-tuning of the
# shipped number fails this gate rather than passing it quietly.
export AW_PING_ATTEMPTS=26

# The stubs. Each records its whole argv and succeeds. `ip netns exec ... tc
# ...` is recorded by the `ip` stub as one line, which is the form the
# assertions below read.
#
# THE PING OUTCOME LIVES IN THE `ip` STUB, not in the `ping` stub, and that is
# a fact about the code under test rather than a shortcut: the sanity check is
# issued as `ip netns exec <ns> ping ...`, so in this harness `ip` is the
# process whose exit status the retry loop actually sees. A `ping` stub is
# never execed at all. `$STUB/ping_plan` steers it:
#
#   (absent)   every ping succeeds                      — the healthy topology
#   <integer>  the first N ping invocations FAIL, then they succeed
#              — the LOSS DRAW: a leg that is up but drops a burst
#   always     every ping invocation fails              — the DEAD LEG
#
# The counter is global across legs, which is the stricter arrangement: with
# `ping_plan = 3` the FIRST leg must burn four attempts before it clears, so a
# retry loop that reset per leg or gave up early would still be caught.
mkstubs() {
    local tool
    for tool in tc sysctl ping; do
        cat > "$STUB/$tool" <<STUBEOF
#!/bin/bash
echo "$tool \$*" >> "$LOG"
exit 0
STUBEOF
        chmod +x "$STUB/$tool"
    done
    cat > "$STUB/ip" <<'STUBEOF'
#!/bin/bash
echo "ip $*" >> "$STUB_LOG"
for a in "$@"; do
    if [ "$a" = ping ]; then
        [ -f "$STUB_DIR/ping_plan" ] || exit 0
        plan=$(cat "$STUB_DIR/ping_plan")
        n=$(( $(cat "$STUB_DIR/ping_count" 2>/dev/null || echo 0) + 1 ))
        echo "$n" > "$STUB_DIR/ping_count"
        if [ "$plan" = always ]; then
            echo "ping: connect: Network is unreachable" >&2
            exit 1
        fi
        if [ "$n" -le "$plan" ]; then
            echo "1 packets transmitted, 0 received, 100% packet loss"
            exit 1
        fi
        echo "1 packets transmitted, 1 received, 0% packet loss"
        exit 0
    fi
done
exit 0
STUBEOF
    chmod +x "$STUB/ip"
}
export STUB_DIR="$STUB" STUB_LOG="$LOG"
mkstubs
export PATH="$STUB:$PATH"

ping_plan() { # (none) | <n> | always
    rm -f "$STUB/ping_plan" "$STUB/ping_count"
    [ $# -eq 1 ] && printf '%s' "$1" > "$STUB/ping_plan"
}

: > "$LOG"
OUT=$(bash ./topo_quad.sh up c2 c2 c3 c3 --seed 42 2>&1)
RC=$?

FAIL=0
ck() { # description  expected_count  pattern
    local desc="$1" want="$2" pat="$3"
    local got
    got=$(grep -cE "$pat" "$LOG")
    if [[ "$got" == "$want" ]]; then
        printf 'ok    %-58s (%s)\n' "$desc" "$got"
    else
        printf 'FAIL  %-58s want %s got %s   /%s/\n' "$desc" "$want" "$got" "$pat"
        FAIL=1
    fi
}

echo "--- topo_quad.sh up c2 c2 c3 c3 --seed 42   (rc=$RC)"
[[ "$RC" == "0" ]] || { echo "FAIL  script exited $RC"; echo "$OUT"; FAIL=1; }

# ── THE GUARD AUDIT — the check that protects SSH to the test VM ──────────
# Nothing outside the rp-* namespaces and the cli*/srv* veth devices may be
# named in ANY issued command. This is asserted as an absence over the whole
# log rather than per call site, so a shaping call added later without its
# guard is caught by this file rather than by a lost SSH session.
if grep -qE '\bens18\b|dev (ens|eth|enp|wlan)' "$LOG"; then
    echo "FAIL  the command stream names a NON-rp device:"
    grep -nE '\bens18\b|dev (ens|eth|enp|wlan)' "$LOG"
    FAIL=1
else
    echo "ok    no command names ens18 or any host NIC"
fi
if grep -E 'netns (add|del|exec)' "$LOG" | grep -qvE 'netns (add|del|exec) rp-(cli|srv)\b'; then
    echo "FAIL  a namespace operation targets a non-rp-* namespace:"
    grep -E 'netns (add|del|exec)' "$LOG" | grep -vE 'netns (add|del|exec) rp-(cli|srv)\b'
    FAIL=1
else
    echo "ok    every netns operation targets rp-cli or rp-srv only"
fi
# `qdisc add` may only ever land on a cli*/srv* veth.
if grep -E 'qdisc add' "$LOG" | grep -qvE 'dev (cli|srv)[0-3] '; then
    echo "FAIL  a qdisc lands on a device that is not a cli*/srv* veth:"
    grep -E 'qdisc add' "$LOG" | grep -vE 'dev (cli|srv)[0-3] '
    FAIL=1
else
    echo "ok    every qdisc lands on a cli[0-3]/srv[0-3] veth"
fi

# ── THE WIDENING AUDIT — four legs, everywhere, not two ───────────────────
ck "four veth pairs created"                4 'ip link add cli[0-3] netns rp-cli type veth peer name srv[0-3] netns rp-srv'
ck "four client addresses"                  4 'ip -n rp-cli addr add 10\.(77|78|79|80)\.0\.1/24'
ck "four server addresses"                  4 'ip -n rp-srv addr add 10\.(77|78|79|80)\.0\.2/24'
ck "four DATA-direction qdiscs (cli*)"      4 'netns exec rp-cli tc qdisc add dev cli[0-3] '
ck "four ACK-direction qdiscs (srv*)"       4 'netns exec rp-srv tc qdisc add dev srv[0-3] '
ck "three extra MPTCP subflow endpoints"    3 'ip mptcp endpoint add 10\.(78|79|80)\.0\.1 dev cli[1-3] subflow'
ck "three extra MPTCP signal endpoints"     3 'ip mptcp endpoint add 10\.(78|79|80)\.0\.2 dev srv[1-3] signal'
ck "MPTCP limits widened to 4, both ns"     2 'ip mptcp limits set subflow 4 add_addr_accepted 4'
ck "four sanity pings, one per leg"         4 'netns exec rp-cli ping -c 1 .* 10\.(77|78|79|80)\.0\.2$'
# THE REPAIR ITSELF, asserted on the healthy path: ONE packet per attempt and
# ONE attempt per leg. The old check spent two packets and a 0.2 s interval on
# every leg unconditionally; the repaired one is strictly cheaper when nothing
# is wrong, which is the case that runs on every invocation of every battery.
ck "exactly 4 ping calls (no retry when clean)" 4 'netns exec rp-cli ping '

# ── THE SEED AUDIT — the defect this cell inherits the repair for ─────────
# Per-leg DERIVED seeds on the data direction: 42, 1042, 2042, 3042. If this
# reads four identical seeds the shared-seed defect has been reintroduced and
# every symmetric-quad correlation would be pinned at +1 by construction.
ck "leg 0 data seed = 42"                   1 'qdisc add dev cli0 .* seed 42$'
ck "leg 1 data seed = 1042"                 1 'qdisc add dev cli1 .* seed 1042$'
ck "leg 2 data seed = 2042"                 1 'qdisc add dev cli2 .* seed 2042$'
ck "leg 3 data seed = 3042"                 1 'qdisc add dev cli3 .* seed 3042$'
ck "FOUR DISTINCT data seeds (not one)"     4 'qdisc add dev cli[0-3] .* seed (42|1042|2042|3042)$'
# The reverse direction takes NO seed — the kernel draws its own. That is the
# seed audit's own control and must survive the widening.
if grep -E 'qdisc add dev srv[0-3] ' "$LOG" | grep -q 'seed '; then
    echo "FAIL  an ACK-direction qdisc carries a seed (it must not — that is the audit's control)"
    FAIL=1
else
    echo "ok    no ACK-direction qdisc carries a seed (the control is preserved)"
fi

# ── THE GEOMETRY — c9h's heterogeneous legs are actually heterogeneous ────
# c2 = 100mbit/5ms/3ms jitter, c3 = 20mbit/20ms/5ms jitter. Two of each.
ck "two c2-class data legs (100mbit, 5ms)"  2 'qdisc add dev cli[0-3] root netem delay 5ms 3ms rate 100mbit'
ck "two c3-class data legs (20mbit, 20ms)"  2 'qdisc add dev cli[0-3] root netem delay 20ms 5ms rate 20mbit'

# ── THE PINNED-SEED ARM — the rho_loss = +1 end of the dial still reachable ─
: > "$LOG"
bash ./topo_quad.sh up c2 c2 c2 c2 --seed 7,7,7,7 >/dev/null 2>&1
ck "explicit equal seeds pin all four legs"  4 'qdisc add dev cli[0-3] .* seed 7$'

# ── THE SHORT-LIST ABORT — a partially derived quad must be refused ───────
: > "$LOG"
if bash ./topo_quad.sh up c2 c2 c2 c2 --seed 7,7 >/dev/null 2>&1; then
    echo "FAIL  --seed 7,7 at a QUAD was accepted; it would mix (7,7,2007,3007)"
    FAIL=1
else
    echo "ok    a short seed list at a quad is REFUSED, not silently derived"
fi

# ── THE SYMMETRIC QUAD (c9 proper), for completeness ──────────────────────
: > "$LOG"
bash ./topo_quad.sh up c2 c2 c2 c2 --seed 42 >/dev/null 2>&1
ck "c9: four identical c2 legs"             4 'qdisc add dev cli[0-3] root netem delay 5ms 3ms rate 100mbit'
ck "c9: four DISTINCT seeds"                4 'qdisc add dev cli[0-3] .* seed (42|1042|2042|3042)$'

# ══ topo_dual.sh — THE ERA BOUNDARY, on the script the defect was found in ══
#
# The quad is new and inherits per-leg seeds by construction. The DUAL is
# where the shared-seed defect actually lived, and where a regression would be
# invisible: `--seed 42` on both legs produced a perfectly normal-looking
# capture for a whole harness era. These checks are the ones that would have
# caught it, so they are written against the repaired script and left standing.
echo
echo "=== topo_dual.sh (the script the shared-seed defect was found in)"

: > "$LOG"
OUT=$(bash ./topo_dual.sh up c2 c2 --seed 42 2>&1); RC=$?
[[ "$RC" == "0" ]] || { echo "FAIL  topo_dual.sh exited $RC"; echo "$OUT"; FAIL=1; }

# THE DEFECT ITSELF: the two legs must NOT share a seed at a symmetric cell.
ck "dual leg 0 seed = 42"                   1 'qdisc add dev cli0 .* seed 42$'
ck "dual leg 1 seed = 1042 (NOT 42)"        1 'qdisc add dev cli1 .* seed 1042$'
if [[ "$(grep -cE 'qdisc add dev cli[01] .* seed 42$' "$LOG")" == "2" ]]; then
    echo "FAIL  REGRESSION: both dual legs carry seed 42 — the shared-seed defect is back"
    FAIL=1
else
    echo "ok    the two dual legs carry DIFFERENT seeds (the defect stays fixed)"
fi
# The ACK direction still takes no seed — the audit's own control.
if grep -E 'qdisc add dev srv[01] ' "$LOG" | grep -q 'seed '; then
    echo "FAIL  a dual ACK-direction qdisc carries a seed"
    FAIL=1
else
    echo "ok    dual ACK direction still takes no seed (the control survives)"
fi
# And the guards, on the unchanged script.
if grep -qE '\bens18\b|dev (ens|eth|enp|wlan)' "$LOG"; then
    echo "FAIL  topo_dual.sh names a non-rp device"; FAIL=1
else
    echo "ok    topo_dual.sh names no host NIC"
fi

# THE rho = +1 ARM must still be exactly reachable — the era boundary is a
# DIAL with two ends, and the old behaviour is how a pre-boundary ledger gets
# re-run for comparison.
: > "$LOG"
bash ./topo_dual.sh up c2 c2 --seed 42,42 >/dev/null 2>&1
ck "dual --seed 42,42 pins BOTH legs (the rho=+1 arm)"  2 'qdisc add dev cli[01] .* seed 42$'

# c8, the ASYMMETRIC cell: different GE params, so it was never affected the
# same way — but it must still get per-leg seeds now.
: > "$LOG"
bash ./topo_dual.sh up c2 c3 --seed 42 >/dev/null 2>&1
ck "c8 keeps its asymmetric params (100mbit + 20mbit)"  1 'qdisc add dev cli0 root netem delay 5ms 3ms rate 100mbit'
ck "c8 second leg is the c3 class"                      1 'qdisc add dev cli1 root netem delay 20ms 5ms rate 20mbit'
ck "c8 legs also get distinct seeds"                    2 'qdisc add dev cli[01] .* seed (42|1042)$'


# ══ THE TOPO-PING REPAIR — a loss draw is not an abort, a dead leg is ══════
#
# THE DEFECT THIS GATE STANDS AGAINST. The era battery's abort-cause witness
# resolved **ALL 38 of its 204 aborts** to `topo_step` at `topo_dual.sh:95/96`
# — the sanity pings — with `topo_step` on 38 of 38 aborts and **0 of 166
# non-aborts**, every one recording `2 packets transmitted, 0 received, 100%
# packet loss`. The engine binary was never launched on any of them. A
# 2-packet no-retry ICMP check across a deliberately Gilbert-Elliott-lossy leg
# aborts on the draw where both packets land in the bad state, and the check's
# purpose is "namespace and route exist", not "zero loss".
#
# The repair is a bounded retry in `aw_ping`. These two cases pin its two
# ends, and they are the reason it is a retry LOOP rather than a wider `-c`:
# only a loop's semantics are observable through a stubbed `ping`.
echo
echo "=== THE TOPO-PING REPAIR (aw_ping's bounded retry)"
# `aw_cause` is FIRST-WRITE-WINS over the whole record file, so the cases above
# (the short-seed-list refusal in particular) have already written one. Start a
# clean record here, or these assertions would be reading that one.
rm -f "$AW_FILE"

# THE SIZING CONSTANT ITSELF. The retry bound is not a matter of taste — it is
# sized so the false-abort probability at the WORST committed GE cell (c5:
# p=5.3 q=30 => pi_bad = 0.15014, persistence 1-q = 0.70) is under 1e-4:
#   P(N draws all lost) = pi_bad * (1-q)^(N-1)
#   N = 2  -> 0.15014 * 0.70    = 1.05e-1   the shipped value, i.e. the class
#   N = 26 -> 0.15014 * 0.70^25 = 2.01e-5   per leg
#                     4 legs    = 8.05e-5   per quad invocation, < 1e-4
# A later edit that quietly shrinks the bound re-opens the class, so the
# DEFAULT is asserted against the file rather than against the environment.
if grep -qE '^: "\$\{AW_PING_ATTEMPTS:=26\}"' abort_witness.sh; then
    echo "ok    aw_ping's default retry bound is still 26 (the sized value)"
else
    echo "FAIL  AW_PING_ATTEMPTS default is no longer 26 — the sizing arithmetic in"
    echo "      abort_witness.sh puts the c5 false-abort probability at 2.0e-5 per leg"
    echo "      ONLY at 26. Re-derive it in the file before changing it here."
    FAIL=1
fi

# CASE 1 — A LOSS DRAW MUST NOT ABORT. The first 3 ping invocations fail, so
# leg 0 needs FOUR attempts before it clears; legs 1-3 then pass first try.
# This is exactly the 38/204 class, and it must now complete with rc 0.
: > "$LOG"; ping_plan 3
OUT=$(bash ./topo_quad.sh up c2 c2 c2 c2 --seed 42 2>&1); RC=$?
if [[ "$RC" == "0" ]]; then
    echo "ok    quad: a ping that fails 3× then succeeds does NOT abort (rc=0)"
else
    echo "FAIL  quad: a transient ping loss draw still aborts (rc=$RC) — THE 38/204 CLASS"
    echo "$OUT"
    FAIL=1
fi
# And it must have RETRIED rather than shrugged: 4 legs + 3 burned draws = 7.
ck "quad: 7 ping attempts (4 legs + 3 retries)"  7 'netns exec rp-cli ping '

: > "$LOG"; ping_plan 3
OUT=$(bash ./topo_dual.sh up c2 c2 --seed 42 2>&1); RC=$?
if [[ "$RC" == "0" ]]; then
    echo "ok    dual: a ping that fails 3× then succeeds does NOT abort (rc=0)"
else
    echo "FAIL  dual: a transient ping loss draw still aborts (rc=$RC) — THE 38/204 CLASS"
    echo "$OUT"
    FAIL=1
fi
ck "dual: 5 ping attempts (2 legs + 3 retries)"  5 'netns exec rp-cli ping '

# CASE 2 — A GENUINELY DEAD LEG MUST STILL ABORT. This is the half the repair
# must not trade away: retrying forever would turn a missing namespace, a
# missing address or a missing route into a silent pass, and the invocation
# would then fail later somewhere far less legible.
: > "$LOG"; ping_plan always
OUT=$(bash ./topo_quad.sh up c2 c2 c2 c2 --seed 42 2>&1); RC=$?
if [[ "$RC" != "0" ]]; then
    echo "ok    quad: an ALWAYS-failing ping still aborts (rc=$RC) — a dead leg is a dead leg"
else
    echo "FAIL  quad: a dead leg was ACCEPTED. The retry swallowed a real failure."
    FAIL=1
fi
# BOUNDED, not unbounded: it gave up after exactly AW_PING_ATTEMPTS draws on
# the first leg and did not go on to the other three.
ck "quad: the retry is BOUNDED at 26 draws"      26 'netns exec rp-cli ping '

: > "$LOG"; ping_plan always
OUT=$(bash ./topo_dual.sh up c2 c2 --seed 42 2>&1); RC=$?
if [[ "$RC" != "0" ]]; then
    echo "ok    dual: an ALWAYS-failing ping still aborts (rc=$RC)"
else
    echo "FAIL  dual: a dead leg was ACCEPTED. The retry swallowed a real failure."
    FAIL=1
fi
ck "dual: the retry is BOUNDED at 26 draws"      26 'netns exec rp-cli ping '

# THE WITNESS STILL RECORDS. The repair must not have cost the instrument that
# found the defect: rc, the ping output, and the NEW attempts column.
if grep -q '^ping_path0_rc=' "$AW_FILE" && grep -q '^ping_path0_attempts=26' "$AW_FILE"; then
    echo "ok    the witness still records the ping rc, and now its attempt count"
else
    echo "FAIL  aw_ping's recording semantics were lost in the repair:"
    grep -E '^(ping_|abort_)' "$AW_FILE" 2>/dev/null | head
    FAIL=1
fi
# The quad's NEW witness wiring (goal-gate c9 contract §6 step 3): a quad that
# cannot name its own failing line is the hardest topology in the tree to debug.
if grep -q '^abort_cause=topo_step' "$AW_FILE"; then
    echo "ok    the quad's ERR trap named the failing step (abort_cause=topo_step)"
else
    echo "FAIL  topo_quad.sh did not record an abort cause — the ERR trap is not wired"
    FAIL=1
fi

ping_plan   # restore the healthy stub for anything added after this point

echo
if [[ "$FAIL" == "0" ]]; then
    echo "TOPO GATE (dual + quad): PASS"
else
    echo "TOPO GATE (dual + quad): FAIL"
fi
exit "$FAIL"
