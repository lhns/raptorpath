#!/bin/bash
# Status section 46: the netem builder is byte-for-byte what `shape` issued before the refactor, and `reshape` re-issues that same
# full line with only the delay changed, via `tc qdisc change` (never add / replace, never the truth counter).
# Runs offline: `ip` is a stub that records its argv, one line per call.
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE" || exit 2
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/bin"
cat > "$TMP/bin/ip" <<'EOF'
#!/bin/bash
printf '%s\n' "$*" >> "$IP_LOG"
EOF
chmod +x "$TMP/bin/ip"
export IP_LOG="$TMP/ip.log"
export PATH="$TMP/bin:$PATH"
FAILS=0
ck() { # name ok
    if [ "$2" = 1 ]; then echo "ok   $1"; else echo "FAIL $1"; FAILS=$((FAILS + 1)); fi
}

# The pre-refactor `shape` (topo_dual.sh at diag36 a037422), verbatim except for the name.
cat > "$TMP/old_shape.sh" <<'EOF'
shape_old() { # ns dev scenario seed
    local ns="$1" dev="$2" scenario="$3" seed="${4:-}"
    read -r rate one_way jitter ge_p ge_q <<< "$(scenario_params "$scenario")"
    local jit=""
    [[ "$jitter" != "0" ]] && jit="${jitter}ms"
    local loss=""
    [[ "$ge_p" != "0" ]] && loss="loss gemodel ${ge_p}% ${ge_q}%"
    local sd=""
    [[ -n "$seed" ]] && sd="seed $seed"
    # shellcheck disable=SC2086
    local extra child
    extra="$(scenario_netem_extra "$scenario")"
    child="$(scenario_child_qdisc "$scenario")"
    local h=""
    [[ -n "$child" ]] && h="handle 1:"
    ip netns exec "$ns" tc qdisc add dev "$dev" root $h netem \
        delay "${one_way}ms" $jit rate "$rate" $loss $sd $extra
    if [[ -n "$child" ]]; then
        ip netns exec "$ns" tc qdisc add dev "$dev" parent 1: handle 10: $child
    fi
    truth_counter_install "$ns" "$dev"
}
EOF

for sc in c1 c2 c3 c3hg c4 c5 clean c2slot c2tbf c2r10; do
    for sd in "" 42 1042; do
        : > "$IP_LOG"
        ( source ./lib.sh; source "$TMP/old_shape.sh"; shape_old rp-cli cli1 "$sc" "$sd" ) >/dev/null 2>&1
        cp "$IP_LOG" "$TMP/old.txt"
        : > "$IP_LOG"
        bash ./topo_dual.sh shape rp-cli cli1 "$sc" "$sd" >/dev/null 2>&1
        cp "$IP_LOG" "$TMP/new.txt"
        if [ -s "$TMP/old.txt" ] && cmp -s "$TMP/old.txt" "$TMP/new.txt"; then ok=1; else ok=0; fi
        ck "shape issues byte-identical commands to the pre-refactor shape: scenario=$sc seed='${sd}'" "$ok"
    done
done

# The pinned lines (the status section 46 test).
got="$(source ./lib.sh; netem_args c3 1042)"
ck "netem_args c3 1042 == 'delay 20ms 5ms rate 20mbit loss gemodel 2% 40% seed 1042'" "$([ "$got" = "delay 20ms 5ms rate 20mbit loss gemodel 2% 40% seed 1042" ] && echo 1 || echo 0)"
got="$(source ./lib.sh; netem_args c3 1042 1520)"
ck "netem_args c3 1042 1520 is that line with 'delay 1520ms'" "$([ "$got" = "delay 1520ms 5ms rate 20mbit loss gemodel 2% 40% seed 1042" ] && echo 1 || echo 0)"
got="$(source ./lib.sh; netem_args c2 42)"
ck "netem_args c2 42 == 'delay 5ms 3ms rate 100mbit loss gemodel 1.3% 50% seed 42'" "$([ "$got" = "delay 5ms 3ms rate 100mbit loss gemodel 1.3% 50% seed 42" ] && echo 1 || echo 0)"
got="$(source ./lib.sh; a="$(netem_args c3 1042)"; b="$(netem_args c3 1042 20)"; [ "$a" = "$b" ] && echo same)"
ck "netem_args with delay_ms = the scenario's own one-way delay is the unchanged line" "$([ "$got" = same ] && echo 1 || echo 0)"
got="$(source ./lib.sh; scenario_one_way_ms c3; scenario_one_way_ms c2)"
ck "scenario_one_way_ms c3 / c2 = 20 / 5" "$([ "$got" = "$(printf '20\n5')" ] && echo 1 || echo 0)"

# reshape
: > "$IP_LOG"
bash ./topo_dual.sh reshape rp-cli cli1 c3 1042 1520 >/dev/null 2>&1
rc=$?
ck "reshape exits 0" "$([ "$rc" = 0 ] && echo 1 || echo 0)"
ck "reshape issues exactly one command: tc qdisc change with the FULL line and only the delay changed" \
   "$([ "$(cat "$IP_LOG")" = "netns exec rp-cli tc qdisc change dev cli1 root netem delay 1520ms 5ms rate 20mbit loss gemodel 2% 40% seed 1042" ] && echo 1 || echo 0)"
ck "reshape never runs add, replace, delete, clsact or a filter (the truth counter is untouched)" \
   "$(grep -qE 'qdisc (add|replace|del)|clsact|filter' "$IP_LOG" && echo 0 || echo 1)"
: > "$IP_LOG"
bash ./topo_dual.sh reshape rp-cli cli1 c3 1042 20 >/dev/null 2>&1
shape_line="$(source ./lib.sh; netem_args c3 1042)"
ck "reshape back to the scenario delay re-issues the original 'shape' words (restore is exact)" \
   "$([ "$(cat "$IP_LOG")" = "netns exec rp-cli tc qdisc change dev cli1 root netem $shape_line" ] && echo 1 || echo 0)"
: > "$IP_LOG"
bash ./topo_dual.sh reshape rp-cli cli0 c2tbf 7 1005 >/dev/null 2>&1
ck "reshape keeps the 'handle 1:' root of a scenario with a child qdisc and does not touch the child" \
   "$([ "$(cat "$IP_LOG")" = "netns exec rp-cli tc qdisc change dev cli0 root handle 1: netem delay 1005ms 3ms rate 100mbit loss gemodel 1.3% 50% seed 7" ] && echo 1 || echo 0)"
: > "$IP_LOG"
bash ./topo_dual.sh reshape other cli1 c3 1 1520 >/dev/null 2>&1
ck "reshape refuses a namespace that is not rp-*" "$([ ! -s "$IP_LOG" ] && echo 1 || echo 0)"
: > "$IP_LOG"
bash ./topo_dual.sh reshape rp-cli ens18 c3 1 1520 >/dev/null 2>&1
ck "reshape refuses the management interface" "$([ ! -s "$IP_LOG" ] && echo 1 || echo 0)"
: > "$IP_LOG"
bash ./topo_dual.sh reshape rp-cli cli1 c3 1 15x >/dev/null 2>&1
ck "reshape refuses a non-integer delay" "$([ ! -s "$IP_LOG" ] && echo 1 || echo 0)"

if [ "$FAILS" = 0 ]; then echo "ALL OK"; exit 0; fi
echo "FAILED $FAILS"
exit 1
