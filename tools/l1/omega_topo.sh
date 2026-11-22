#!/bin/bash
# Namespaces, qdiscs and sysctls of the disorder-price (omega) harness. SOURCE it; it defines functions and constants only.
# Registration: docs/derivations/disorder-price-registration.md section 4 (topology, reorderer) and 5 (inner stack).
#
# VM SAFETY (the guards below are the only way this file touches the network):
#   * only network namespaces named rp-*: every ip / tc / ethtool / sysctl / tcpdump / iperf3 / ss call goes through nsx (ip netns exec rp-...) or
#     nsip (ip -n rp-...); omega_guard_ns refuses any other name and EXITS the caller with ABORT-NS (exit 6);
#   * only the veth ends this file creates (OMEGA_DEVS) and the TUN pair of stage T are ever named; omega_guard_dev refuses anything else,
#     ens18 (OMEGA_MGMT_IF) and lo included. Nothing here touches the root namespace's devices, the firewall (no nft, no iptables) or sshd;
#   * no modprobe, no pkill -f, no killall, no kill without a recorded PID;
#   * no root-namespace sysctl is written; the root tcp_* values are only READ (hoststate_snapshot / hoststate_check).
# Call the guards at the top level of the script, never inside $( ): an exit there would end only the subshell.

OMEGA_SUDO="${OMEGA_SUDO:-sudo -n}"
OMEGA_MGMT_IF="${OMEGA_MGMT_IF:-ens18}"
# Overridable only so that a misconfiguration is REFUSED (omega_check_names, at launch, before any lock or command).
OMEGA_NS_CLI="${OMEGA_NS_CLI:-rp-cli}"
OMEGA_NS_MID="${OMEGA_NS_MID:-rp-mid}"
OMEGA_NS_SRV="${OMEGA_NS_SRV:-rp-srv}"
OMEGA_NS_APP="${OMEGA_NS_APP:-rp-app}"
OMEGA_NS_PF=rp-pf          # the pre-flight namespace of the sysctl presence check
OMEGA_DEVS=" cli0 mid0 mid1 srv0 srv1 app0 rpcli0 rpsrv0 "
OMEGA_PROCFS="${OMEGA_PROCFS:-/proc}"

# The inner stack D of registration section 5: key value. tcp_no_metrics_save = 1 in every row.
OMEGA_SYSCTL_D=(
  "tcp_congestion_control cubic" "tcp_recovery 1" "tcp_reordering 3" "tcp_max_reordering 300" "tcp_sack 1" "tcp_dsack 1"
  "tcp_timestamps 1" "tcp_frto 2" "tcp_early_retrans 3" "tcp_ecn 2" "tcp_no_metrics_save 1"
)

omega_guard_ns() { # name  -- refuse (exit 6, ABORT-NS) anything but rp-<word>
  case "$1" in
    rp-*) case "${1#rp-}" in ''|*[!A-Za-z0-9_-]*) echo "ABORT-NS REFUSED: namespace '$1' has an unsafe name" >&2; exit 6 ;; esac ;;
    *) echo "ABORT-NS REFUSED: namespace '$1' is not rp-* prefixed" >&2; exit 6 ;;
  esac
}

omega_check_names() { # launch-time: every configured namespace name must be rp-*; refuses (exit 6) before anything is run
  local ns
  for ns in $OMEGA_NS_CLI $OMEGA_NS_MID $OMEGA_NS_SRV $OMEGA_NS_APP $OMEGA_NS_PF; do omega_guard_ns "$ns"; done
}

omega_guard_dev() { # name  -- refuse (exit 6, ABORT-NS) the management interface and anything this harness did not create
  if [ "$1" = "$OMEGA_MGMT_IF" ] || [ "$1" = lo ]; then
    echo "ABORT-NS REFUSED: will not touch device '$1' (management / loopback / host side of $OMEGA_MGMT_IF)" >&2; exit 6
  fi
  case "$OMEGA_DEVS" in *" $1 "*) ;; *) echo "ABORT-NS REFUSED: device '$1' is not one of the harness's veth / TUN ends" >&2; exit 6 ;; esac
}

nsx() { # ns cmd...   run cmd inside rp-<ns>
  local ns="$1"; shift
  omega_guard_ns "$ns"
  $OMEGA_SUDO ip netns exec "$ns" "$@"
}

nsip() { # ns ip-args...   ip -n rp-<ns> ...
  local ns="$1"; shift
  omega_guard_ns "$ns"
  $OMEGA_SUDO ip -n "$ns" "$@"
}

# PID discipline: $RUN/pids holds "<pid> <start time>" (field 22 of /proc/<pid>/stat). A recorded PID is signalled only while its start time still
# matches (omega_pid_ours), so a PID the kernel reused for another process is never touched. No /proc or no recorded start: never signalled.
omega_pid_start() { # pid -> the process start time (clock ticks since boot), nothing if the process is gone
  local s
  s="$(cat "/proc/$1/stat" 2>/dev/null)" || return 1
  s="${s##*) }"
  set -- $s
  echo "${20:-}"
}
omega_pid_record() { # pid
  [ -z "${RUN:-}" ] || echo "$1 $(omega_pid_start "$1")" >> "$RUN/pids"
}
omega_pid_ours() { # pid -- true iff pid is recorded and still the process that was started
  local rec cur
  rec="$(awk -v p="$1" '$1 == p {s = $2} END {print s}' "${RUN:-/nonexistent}/pids" 2>/dev/null)"
  [ -n "$rec" ] || return 1
  cur="$(omega_pid_start "$1")" || return 1
  [ "$cur" = "$rec" ]
}
omega_signal() { # sig pid -- signal a recorded PID only if it is still ours
  omega_pid_ours "$2" || return 1
  kill "-$1" "$2" 2>/dev/null
}

# A long-running process in rp-<ns>, a DIRECT child of this shell ($! is the sudo process, which relays TERM to the command): so a kill by recorded PID
# reaches it. ns, a log file (stdout; stderr goes to <log>.err), then the command. Sets OMEGA_LASTPID and records it in $RUN/pids.
omega_bg() {
  local ns="$1" lf="$2"
  shift 2
  omega_guard_ns "$ns"
  $OMEGA_SUDO ip netns exec "$ns" "$@" > "$lf" 2> "$lf.err" &
  OMEGA_LASTPID=$!
  omega_pid_record "$OMEGA_LASTPID"
}

ns_add() { omega_guard_ns "$1"; $OMEGA_SUDO ip netns add "$1"; }
ns_del() { omega_guard_ns "$1"; $OMEGA_SUDO ip netns del "$1" 2>/dev/null || true; }

omega_ns_all() { echo "$OMEGA_NS_CLI $OMEGA_NS_MID $OMEGA_NS_SRV $OMEGA_NS_APP $OMEGA_NS_PF"; }

# ── topology: rp-cli cli0 == mid0 rp-mid mid1 == srv0 rp-srv (== srv1 app0 rp-app in stage T) ──
topo_down() { # teardown never acts on a name outside rp-*: it skips it (the launch-time check already refused the session)
  local ns
  for ns in $(omega_ns_all); do
    case "$ns" in rp-*) ns_del "$ns" ;; esac
  done
}

topo_up() {
  local ns d
  topo_down
  for ns in $OMEGA_NS_CLI $OMEGA_NS_MID $OMEGA_NS_SRV; do ns_add "$ns" || return 1; done
  nsip $OMEGA_NS_CLI link add cli0 type veth peer name mid0 netns $OMEGA_NS_MID || return 1
  nsip $OMEGA_NS_MID link add mid1 type veth peer name srv0 netns $OMEGA_NS_SRV || return 1
  nsip $OMEGA_NS_CLI addr add 10.77.1.1/24 dev cli0
  nsip $OMEGA_NS_MID addr add 10.77.1.2/24 dev mid0
  nsip $OMEGA_NS_MID addr add 10.77.2.1/24 dev mid1
  nsip $OMEGA_NS_SRV addr add 10.77.2.2/24 dev srv0
  for ns in $OMEGA_NS_CLI $OMEGA_NS_MID $OMEGA_NS_SRV; do nsip "$ns" link set lo up; done
  nsip $OMEGA_NS_CLI link set cli0 up; nsip $OMEGA_NS_MID link set mid0 up; nsip $OMEGA_NS_MID link set mid1 up; nsip $OMEGA_NS_SRV link set srv0 up
  nsip $OMEGA_NS_CLI route add 10.77.2.0/24 via 10.77.1.2
  nsip $OMEGA_NS_SRV route add 10.77.1.0/24 via 10.77.2.1
  nsx $OMEGA_NS_MID sysctl -w net.ipv4.ip_forward=1 >/dev/null || return 1     # forwarding in rp-mid only
  for d in "$OMEGA_NS_CLI cli0" "$OMEGA_NS_MID mid0" "$OMEGA_NS_MID mid1" "$OMEGA_NS_SRV srv0"; do
    set -- $d; omega_guard_dev "$2"
    nsx "$1" ethtool -K "$2" tso off gso off gro off >/dev/null || return 1     # one skb = one packet
  done
  echo "TOPO-UP rp-cli:cli0 == rp-mid:mid0,mid1 == rp-srv:srv0"
}

# Stage T adds rp-app behind rp-srv. The reorderer then sits on srv1 (rp-srv egress to rp-app); forwarding is enabled in rp-srv (inside rp-*).
topo_up_app() {
  ns_add $OMEGA_NS_APP || return 1
  nsip $OMEGA_NS_SRV link add srv1 type veth peer name app0 netns $OMEGA_NS_APP || return 1
  nsip $OMEGA_NS_SRV addr add 10.88.0.1/24 dev srv1
  nsip $OMEGA_NS_APP addr add 10.88.0.2/24 dev app0
  nsip $OMEGA_NS_SRV link set srv1 up; nsip $OMEGA_NS_APP link set app0 up; nsip $OMEGA_NS_APP link set lo up
  nsip $OMEGA_NS_APP route add default via 10.88.0.1
  nsx $OMEGA_NS_SRV sysctl -w net.ipv4.ip_forward=1 >/dev/null || return 1
  nsx $OMEGA_NS_SRV ethtool -K srv1 tso off gso off gro off >/dev/null
  nsx $OMEGA_NS_APP ethtool -K app0 tso off gso off gro off >/dev/null
}

topo_status() {
  local ns
  for ns in $OMEGA_NS_CLI $OMEGA_NS_MID $OMEGA_NS_SRV $OMEGA_NS_APP; do
    $OMEGA_SUDO ip netns list 2>/dev/null | grep -q "^$ns\b" && echo "NS-PRESENT $ns"
  done
  return 0
}

# ── qdiscs ──
rate_bit() { echo $(( $1 * 12000 ))bit; }       # packets/s of 1500 B -> bit/s

q_bottleneck() { # g limit      netem rate on cli0 egress, FIFO, no delay: the limit counts the queue alone
  omega_guard_dev cli0
  nsx $OMEGA_NS_CLI tc qdisc replace dev cli0 root netem rate "$(rate_bit "$1")" limit "$2"
}
q_rtt() { # rtt_ms             the whole propagation delay on the ACK leg (srv0 egress), no rate
  omega_guard_dev srv0
  nsx $OMEGA_NS_SRV tc qdisc replace dev srv0 root netem delay "${1}ms" limit 100000
}
q_reorder() { # ns dev tabledir table kind dt_ms g seed|-      one two-point-table netem; kind REL|HOLD|CLEAN; table = <name> in tabledir (.dist)
  local ns="$1" dev="$2" dir="$3" tab="$4" kind="$5" dt="$6" g="$7" seed="$8" extra="" us
  omega_guard_dev "$dev"
  us=$(awk -v d="$dt" 'BEGIN{printf "%d", d*1000 + 0.5}')
  [ "$kind" != HOLD ] || extra="rate $(rate_bit $(( g * 10 )))"
  [ "$seed" = "-" ] || extra="$extra seed $seed"
  # shellcheck disable=SC2086
  nsx "$ns" env TC_LIB_DIR="$dir" tc qdisc replace dev "$dev" root netem limit 100000 delay 0us "${us}us" distribution "$tab" $extra
}
q_reorder_change() { # same arguments as q_reorder: switch the live netem's table (the warm-up witness)
  local ns="$1" dev="$2" dir="$3" tab="$4" kind="$5" dt="$6" g="$7" seed="$8" extra="" us
  omega_guard_dev "$dev"
  us=$(awk -v d="$dt" 'BEGIN{printf "%d", d*1000 + 0.5}')
  [ "$kind" != HOLD ] || extra="rate $(rate_bit $(( g * 10 )))"
  # shellcheck disable=SC2086
  nsx "$ns" env TC_LIB_DIR="$dir" tc qdisc change dev "$dev" root netem limit 100000 delay 0us "${us}us" distribution "$tab" $extra
}
q_outage_setup() { # prio with a plug in each band on mid1: band 1 = odd IP IDs
  omega_guard_dev mid1
  nsx $OMEGA_NS_MID tc qdisc replace dev mid1 root handle 1: prio bands 2 priomap 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 || return 1
  nsx $OMEGA_NS_MID tc qdisc add dev mid1 parent 1:1 handle 10: plug limit 100000 || return 1
  nsx $OMEGA_NS_MID tc qdisc add dev mid1 parent 1:2 handle 20: plug limit 100000 || return 1
  nsx $OMEGA_NS_MID tc filter add dev mid1 parent 1: protocol ip prio 1 u32 match u16 0x0001 0x0001 at 4 flowid 1:1 || return 1
  nsx $OMEGA_NS_MID tc qdisc change dev mid1 handle 10: plug release_indefinite
  nsx $OMEGA_NS_MID tc qdisc change dev mid1 handle 20: plug release_indefinite
}
q_outage() { # which: REL (band 1) | HOLD (both bands) | NONE ; hold seconds
  local h
  case "$1" in REL) h="10:" ;; HOLD) h="10: 20:" ;; *) return 0 ;; esac
  for q in $h; do nsx $OMEGA_NS_MID tc qdisc change dev mid1 handle "$q" plug block; done
  "${OMEGA_SLEEP:-sleep}" "$2"
  for q in $h; do nsx $OMEGA_NS_MID tc qdisc change dev mid1 handle "$q" plug release_indefinite; done
}

# ── per-namespace sysctls (never the root namespace) ──
sysctl_preflight() { # every key of stack D must exist under /proc/sys/net/ipv4 of a FRESH rp-* namespace, else ABORT-SYSCTL with nothing written
  local kv k bad=""
  ns_add $OMEGA_NS_PF || { echo "ABORT-SYSCTL cannot create $OMEGA_NS_PF"; return 1; }
  for kv in "${OMEGA_SYSCTL_D[@]}"; do
    k="${kv%% *}"
    nsx $OMEGA_NS_PF test -e "/proc/sys/net/ipv4/$k" || bad="$bad $k"
  done
  ns_del $OMEGA_NS_PF
  [ -z "$bad" ] || { echo "ABORT-SYSCTL not per namespace (absent in a fresh rp-* namespace):$bad; nothing written"; return 1; }
  echo "SYSCTL-PREFLIGHT all ${#OMEGA_SYSCTL_D[@]} keys are per namespace"
}

sysctl_apply() { # ns  -- write stack D inside ns and read every value back (rule 15); echo it
  local ns="$1" kv k v got
  for kv in "${OMEGA_SYSCTL_D[@]}"; do
    k="${kv%% *}"; v="${kv#* }"
    nsx "$ns" sysctl -w "net.ipv4.$k=$v" >/dev/null || { echo "ABORT-SYSCTL write net.ipv4.$k in $ns"; return 1; }
    got="$(nsx "$ns" sysctl -n "net.ipv4.$k" 2>/dev/null | tr -s '[:space:]' ' ' | sed 's/ $//')"
    [ "$got" = "$v" ] || { echo "ABORT-SYSCTL readback of net.ipv4.$k in $ns is '$got', not '$v'"; return 1; }
    echo "SYSCTL-ECHO $ns net.ipv4.$k=$got"
  done
}

sysctl_readback_ok() { # ns -- 0 when every stack-D value still reads back as set (a change during a rep is VOID-WITNESS)
  local ns="$1" kv k v got
  for kv in "${OMEGA_SYSCTL_D[@]}"; do
    k="${kv%% *}"; v="${kv#* }"
    got="$(nsx "$ns" sysctl -n "net.ipv4.$k" 2>/dev/null | tr -s '[:space:]' ' ' | sed 's/ $//')"
    [ "$got" = "$v" ] || return 1
  done
}

hoststate_snapshot() { # file -- the root namespace's tcp_* values, READ only
  local f
  for f in "$OMEGA_PROCFS"/sys/net/ipv4/tcp_*; do
    [ -f "$f" ] && echo "$(basename "$f")=$(tr -s '[:space:]' ' ' < "$f" 2>/dev/null)"
  done | LC_ALL=C sort > "$1"
}
hoststate_check() { # snapshot -- ABORT-HOSTSTATE when any root tcp_* value differs from the snapshot
  local now; now="$(mktemp)"
  hoststate_snapshot "$now"
  if ! cmp -s "$1" "$now"; then
    echo "ABORT-HOSTSTATE a root-namespace tcp_* sysctl differs from pre-flight: $(diff "$1" "$now" | grep '^[<>]' | head -4 | tr '\n' ' ')"
    rm -f "$now"; return 1
  fi
  rm -f "$now"
}

tcp_metrics_flush() { # [cli_ns srv_ns]  both ends, before every rep: no carry-over between reps
  nsx "${1:-$OMEGA_NS_CLI}" ip tcp_metrics flush all >/dev/null 2>&1 || true
  nsx "${2:-$OMEGA_NS_SRV}" ip tcp_metrics flush all >/dev/null 2>&1 || true
}
