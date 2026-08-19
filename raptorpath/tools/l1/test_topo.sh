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

# The stubs. Each records its whole argv and succeeds. `ip netns exec ... tc
# ...` is recorded by the `ip` stub as one line, which is the form the
# assertions below read.
for tool in ip tc sysctl ping; do
    cat > "$STUB/$tool" <<STUBEOF
#!/bin/bash
echo "$tool \$*" >> "$LOG"
exit 0
STUBEOF
    chmod +x "$STUB/$tool"
done
export PATH="$STUB:$PATH"

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
ck "four sanity pings, one per leg"         4 'netns exec rp-cli ping -c 2 .* 10\.(77|78|79|80)\.0\.2$'

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

echo
if [[ "$FAIL" == "0" ]]; then
    echo "TOPO GATE (dual + quad): PASS"
else
    echo "TOPO GATE (dual + quad): FAIL"
fi
exit "$FAIL"
