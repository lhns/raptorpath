#!/bin/bash
# Run under sudo: resolve $HOME to the invoking user's home, not root's, so paths and the tree lock match.
HOME="$(getent passwd "${SUDO_USER:-$(id -un)}" | cut -d: -f6)"
# THE DISORDER-PRICE (omega) SESSION ENVELOPE. Registration: docs/derivations/disorder-price-registration.md (v4), sections 4, 5, 10, 11, 12.
# No engine code: the registration measures with netem / prio / plug in rp-* namespaces, iperf3 and tcpdump; stage T and session R's
# standing-queue measurement also run the existing raptorpath binary, pinned by sha.
#
# ── README ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────
# Sessions (OMEGA_SESSION):
#   R              session R (about 2.5 h): R0 (the reorderer alone), tool checks with fixed fallbacks, the standing queue through the real tunnel
#                  (needs OMEGA_BIN), noise calibration (8 configurations x 4 arms x 6 reps), smoke. Scores nothing. Writes AMENDMENT.txt.
#   R + OMEGA_REHEARSE=1   the rehearsal: topology, tables, sysctls, one row per arm type (and the two outage arms), with the whole envelope
#                  around it. No R0, no tunnel, no calibration. Its ledger header says rehearsal and the scorer refuses it.
#   D1 .. Dn       the Linux phase, stack D, in sessions of at most 4.5 h cut at balanced round boundaries; the last one also runs the outage
#                  check and stage T. The Linux phase is capped at 20 h in all (WALL_S of the D runs under OMEGA_ROOT). Needs OMEGA_PINS
#                  (the amendment) and a finished session R (OMEGA_R_RUN with DONE-ALL). The classic stack (phase C) is not built.
# Usage (as the VM user, not root; sudo -n is used only for the measurement commands):
#   OMEGA_ROOT=~/omega-run OMEGA_SESSION=R OMEGA_HARD=<epoch> setsid nohup bash omega_run_all.sh > $OMEGA_ROOT/launch.out 2>&1 < /dev/null &
#   Watch with: wait for DONE-ALL || FAILED-ALL || TRUNCATED-<session> in the run directory (no polling loop of tc / pcaps: rule 13).
# Paths (run directory $RUN = $OMEGA_ROOT/<session>-<utc stamp>):
#   $RUN/ledger.jsonl   header, one JSON line per row, footer {"state": DONE-ALL | FAILED-ALL | TRUNCATED-<session>}. The scorer reads only DONE-ALL.
#   $RUN/rows/<row>/    per-row tc counters before / after, backlog samples, text digests, meta.json (rep 1 keeps its pcaps)
#   $RUN/tables/        netem tables (TC_LIB_DIR; nothing is written under /usr/lib)
#   $RUN/{session.log,all-era.txt,pids,rows.txt,R0.json,QT.json,AMENDMENT.txt,hoststate.pre,WALL_S}   DONE-ALL / FAILED-ALL / TRUNCATED-<session>  sentinels
# Environment:
#   OMEGA_SESSION    R | D<k>                       (required)             OMEGA_REHEARSE   1 = the rehearsal (with R)
#   OMEGA_ROOT       parent of the run directories (default $HOME/omega-run)
#   OMEGA_HARD       epoch seconds, the hard deadline (REQUIRED, an LD_HARD)  OMEGA_STOP_AT   epoch: the stopper asks for a stop at the next
#                    balanced round boundary (default min(launch + 4.9 h, OMEGA_HARD))
#   OMEGA_PINS       the amendment (JSON: B_t_pkts {rate: pkts}, n {cfg: n}, L, seed_ok, stage_t_min_rtt_ms, d_ms, sigma_R_ms, skip_outage)
#   OMEGA_R_RUN      the finished session R's run directory (D sessions)
#   OMEGA_BIN, OMEGA_BIN_SHA   raptorpath for the tunnel measurement and stage T, and the amendment's sha pin
#   OMEGA_LOCK_VM    default /tmp/rwm-vm.lock       OMEGA_LOCK_RP  default ${RWM_RP_LOCK:-$HOME/rp.lock}
#   OMEGA_PYTHON (python3), OMEGA_SUDO (sudo -n), OMEGA_MGMT_IF (ens18), OMEGA_PROCFS (/proc), OMEGA_PROBE (omega_probe.py)
#   OMEGA_ROW_EST_S (41 at Q_t, 19 at Q_s), OMEGA_ROW_KB (4096, the disk budget per row), OMEGA_TINY=1 (the stub run's tiny design; never a launch)
#   time seams, registered defaults: OMEGA_WIN_S 1.0, OMEGA_NOFILL_S 60, OMEGA_EVENT_S 12, OMEGA_SAMPLE_S 0.01, OMEGA_POLL_S 0.25, OMEGA_SETTLE_S 0.5,
#   OMEGA_TUNQ_S 90, OMEGA_SLEEP (sleep)
# Abort strings (registration 10, in its order; each prints `<TOKEN> <reason>` and ends the session with FAILED-ALL):
#   ABORT-LOCK (exit 4)  a lock exists at `test -e`, or take_lock refuses; nothing is run         ABORT-CRLF (3)  a CR byte in a harness script
#   ABORT-TOOLS          iperf3, tcpdump, ethtool, ss, ip or tc missing, or tc refuses a qdisc / table (`plug` missing only skips the outage check)
#   ABORT-BENCH          R0 refutes a reorderer property; no battery is launched
#   ABORT-SYSCTL         a stack-D sysctl is not per namespace (pre-flight, nothing written) or a readback differs
#   ABORT-NS (exit 6)    a namespace or device outside rp-* / the harness's veth ends was asked for (the guards refuse; also ens18 and lo)
#   ABORT-SMOKE          a witness is missing on either end, or a smoke row did not digest
#   ABORT-BUILD          raptorpath's sha differs from OMEGA_BIN_SHA, or stage T has no pinned binary
#   TUN-BRINGUP-FAIL     three attempts exhausted: stage T (or R's tunnel measurement) is skipped, not the session
#   ABORT-DISK           free space below the session's budget at launch      ABORT-HOSTSTATE  a root tcp_* sysctl differs from pre-flight
#   Additions of this harness: ABORT-BUDGET (the session does not fit before OMEGA_HARD, or the Linux phase cap of 20 h) and ABORT-AMENDMENT
#   (a D session without OMEGA_PINS or a finished session R) and ABORT-SESSION (bad arguments).
#   Row outcomes: NO_FILL (60 s without the standing queue), VOID-WITNESS, SKIPPED-TABLE, SKIPPED-SELECTOR, SKIPPED-PLUG. Cuts: TRUNCATE-HARD (a row
#   would end after OMEGA_HARD), TRUNCATE-STOP (the stopper fired; the session ends at a round boundary). A cut run writes the TRUNCATED-<session>
#   marker and a TRUNCATED footer; the scorer reads no result from it.
# VM safety: only rp-* namespaces (omega_topo.sh guards every call); never ens18, the firewall or sshd; no modprobe, nft, iptables; kills by recorded PID
# or `pkill -x raptorpath` only; locks only through take_lock / release_locks, never broken; no VM user name, address or home path in the repo.
# ─────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────

set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE" || { echo "ABORT-CD $HERE"; exit 3; }
PY="${OMEGA_PYTHON:-python3} -I"
OMEGA_LAUNCH="$(date +%s)"
SESSION="${OMEGA_SESSION:-}"
REHEARSAL=0; [ "${OMEGA_REHEARSE:-0}" != 1 ] || REHEARSAL=1
FINAL=""; RUN=""; LEDGER=""; PINS=""; HARD=""; LASTCFG=""; TUNNEL_UP=0; TSKIP=""; TOPO_STARTED=0

# ── the library ──
source ./lib_battery.sh
declare -F take_lock >/dev/null || { echo "ABORT-LIB lib_battery.sh did not load"; exit 3; }
source ./omega_topo.sh
source ./omega_battery.sh
source ./omega_tunnel.sh

omega_abort() { # TOKEN reason...
  echo "$1 ${*:2}"
  [ -z "$RUN" ] || echo "$1 ${*:2}" >> "$RUN/all-era.txt"
  [ -n "$FINAL" ] || FINAL="failed"
  exit 3
}

omega_footer() { # state
  [ -n "$LEDGER" ] && [ -d "$RUN" ] && printf '{"kind":"footer","state":"%s","ts":"%s"}\n' "$1" "$(date -u +%FT%TZ)" >> "$LEDGER"
}

omega_cleanup() {
  local rc=$? p state ok=1
  trap - EXIT INT TERM
  [ -z "$RUN" ] || [ ! -f "$RUN/pids" ] || while read -r p; do
    [ -n "$p" ] && kill -0 "$p" 2>/dev/null && kill -TERM "$p" 2>/dev/null
  done < "$RUN/pids"
  omega_stop_raptorpath
  # give the recorded PIDs a moment to leave (by PID only), then KILL the stragglers by PID
  for p in 1 2 3 4 5 6 7 8 9 10; do
    alive=0
    [ -z "$RUN" ] || [ ! -f "$RUN/pids" ] || while read -r q; do [ -n "$q" ] && kill -0 "$q" 2>/dev/null && alive=1; done < "$RUN/pids"
    [ "$alive" = 0 ] && break
    "${OMEGA_SLEEP:-sleep}" 0.3
  done
  [ "$alive" = 0 ] || { while read -r q; do [ -n "$q" ] && kill -0 "$q" 2>/dev/null && kill -KILL "$q" 2>/dev/null; done < "$RUN/pids"; }
  [ "$TOPO_STARTED" != 1 ] || topo_down
  if [ -n "$RUN" ] && [ -f "$RUN/hoststate.pre" ] && [ "$FINAL" != failed ]; then
    hoststate_check "$RUN/hoststate.pre" || { FINAL="failed"; echo "ABORT-HOSTSTATE (at exit)"; }
  fi
  [ -z "$RUN" ] || [ ! -d "$RUN" ] || echo $(( $(date +%s) - OMEGA_LAUNCH )) > "$RUN/WALL_S"
  case "$FINAL" in
    done) state="DONE-ALL" ;;
    truncated) state="TRUNCATED-$SESSION" ;;
    *) state="FAILED-ALL" ;;
  esac
  if [ -n "$RUN" ] && [ -d "$RUN" ]; then
    omega_footer "$state"
    touch "$RUN/$state"
    # the exit state, verified (12): no raptorpath, no recorded PID alive, no rp-* namespace, both locks released
    pgrep -x raptorpath >/dev/null 2>&1 && { echo "EXIT-STATE raptorpath still running"; ok=0; }
    [ ! -f "$RUN/pids" ] || while read -r p; do [ -n "$p" ] && kill -0 "$p" 2>/dev/null && { echo "EXIT-STATE recorded PID $p alive"; ok=0; }; done < "$RUN/pids"
    $OMEGA_SUDO ip netns list 2>/dev/null | grep -q '^rp-' && { echo "EXIT-STATE an rp-* namespace remains"; ok=0; }
  fi
  release_locks
  for p in "${OMEGA_LOCK_VM:-/nonexistent-lock-a}" "${OMEGA_LOCK_RP:-/nonexistent-lock-b}"; do
    [ ! -e "$p" ] || [ "$LOCKS_TAKEN_BY_US" != 1 ] || { echo "EXIT-STATE lock $p still present"; ok=0; }
  done
  echo "OMEGA-END state=$state exit_state_clean=$ok"
  case "$FINAL" in done) exit "$rc" ;; truncated) exit 5 ;; *) exit $(( rc == 0 ? 3 : rc )) ;; esac
}
LOCKS_TAKEN_BY_US=0

# ── 0. arguments ──
case "$SESSION" in R|D[0-9]|D[0-9][0-9]) ;; *) echo "ABORT-SESSION OMEGA_SESSION must be R or D<k>, not '$SESSION'"; exit 3 ;; esac
[ "$REHEARSAL" != 1 ] || [ "$SESSION" = R ] || { echo "ABORT-SESSION OMEGA_REHEARSE=1 goes with OMEGA_SESSION=R"; exit 3; }
[ "$REHEARSAL" != 1 ] || SESSION_DESIGN="R-rehearsal"
SESSION_DESIGN="${SESSION_DESIGN:-$SESSION}"
OMEGA_LOCK_VM="${OMEGA_LOCK_VM:-/tmp/rwm-vm.lock}"
OMEGA_LOCK_RP="${OMEGA_LOCK_RP:-${RWM_RP_LOCK:-$HOME/rp.lock}}"
OMEGA_ROOT="${OMEGA_ROOT:-$HOME/omega-run}"
case "${OMEGA_HARD:-}" in ''|*[!0-9]*) echo "ABORT-BUDGET OMEGA_HARD (epoch seconds, a hard deadline) is required"; exit 3 ;; esac
HARD="$OMEGA_HARD"
PINS="${OMEGA_PINS:-}"

# ── 1. the locks: `test -e` first, nothing is run if either exists; they are never broken ──
for _l in "$OMEGA_LOCK_VM" "$OMEGA_LOCK_RP"; do
  if test -e "$_l"; then echo "ABORT-LOCK $_l exists ($(cat "$_l" 2>/dev/null)); NOTHING WAS RUN"; exit 4; fi
done

# ── 2. CRLF (rule 10), tools, amendment, budget, disk: all before any lock is taken or any VM command is issued ──
crlf_guard omega_run_all.sh omega_topo.sh omega_battery.sh omega_tunnel.sh omega_sampler.sh omega_cells.py omega_dist.py omega_parse.py omega_score.py omega_probe.py lib.sh lib_battery.sh
omega_check_names
for _t in iperf3 tcpdump ethtool ss ip tc awk; do
  command -v "$_t" >/dev/null 2>&1 || { echo "ABORT-TOOLS $_t is missing; NOTHING WAS RUN"; exit 3; }
done
case "$SESSION" in
  D*)
    [ -n "$PINS" ] && [ -f "$PINS" ] || { echo "ABORT-AMENDMENT a D session needs OMEGA_PINS (the amendment); NOTHING WAS RUN"; exit 3; }
    [ -n "${OMEGA_R_RUN:-}" ] && [ -e "$OMEGA_R_RUN/DONE-ALL" ] || { echo "ABORT-AMENDMENT a D session needs a finished session R (OMEGA_R_RUN with DONE-ALL); NOTHING WAS RUN"; exit 3; }
    $PY omega_cells.py param g1000-rtt10-Qt CLEAN "$PINS" >/dev/null 2>&1 || { echo "ABORT-AMENDMENT $PINS is not a pins file"; exit 3; }
    $PY -c 'import json,sys; p=json.load(open(sys.argv[1])); sys.exit(0 if p.get("B_t_pkts") else 1)' "$PINS" || { echo "ABORT-AMENDMENT $PINS pins no B_t_pkts"; exit 3; }
    ;;
esac
EST="$($PY omega_cells.py budget "$SESSION_DESIGN" $PINS)" || { echo "ABORT-BUDGET cannot size session $SESSION"; exit 3; }
NOW="$(date +%s)"
if [ $(( NOW + EST )) -gt "$HARD" ]; then echo "ABORT-BUDGET session $SESSION needs about ${EST}s; OMEGA_HARD leaves $(( HARD - NOW ))s; NOTHING WAS RUN"; exit 3; fi
case "$SESSION" in D*)
  USED="$(cat "$OMEGA_ROOT"/D*/WALL_S 2>/dev/null | awk '{s+=$1} END{print s+0}')"
  if [ $(( USED + EST )) -gt 72000 ]; then echo "ABORT-BUDGET the Linux phase is capped at 20 h: ${USED}s used, ${EST}s more needed; NOTHING WAS RUN"; exit 3; fi ;;
esac
ROWS_N="$($PY omega_cells.py rows "$SESSION_DESIGN" $PINS | wc -l)"
mkdir -p "$OMEGA_ROOT" 2>/dev/null
NEED_KB=$(( ROWS_N * ${OMEGA_ROW_KB:-4096} ))
HAVE_KB="$(df -Pk "$OMEGA_ROOT" 2>/dev/null | awk 'NR==2{print $4}')"
if [ -z "$HAVE_KB" ] || [ "$HAVE_KB" -lt "$NEED_KB" ]; then echo "ABORT-DISK ${HAVE_KB:-?} KB free under $OMEGA_ROOT, the session needs about $NEED_KB KB; NOTHING WAS RUN"; exit 3; fi

# ── 3. the run directory, sentinels proven writable, the locks ──
RUN="$OMEGA_ROOT/$SESSION-$(date -u +%Y%m%dT%H%M%SZ)$([ "$REHEARSAL" = 1 ] && echo -rehearsal)"
mkdir -p "$RUN/rows" "$RUN/tables" || { echo "ABORT-DISK cannot create $RUN"; exit 3; }
LEDGER="$RUN/ledger.jsonl"; LB_LOG="$RUN/all-era.txt"; LB_TAG="omega-$SESSION"
trap omega_cleanup EXIT
prove_sentinels "$RUN" DONE-ALL FAILED-ALL "TRUNCATED-$SESSION" > "$RUN/sentinels.txt" 2>&1 || { cat "$RUN/sentinels.txt"; exit 3; }
take_lock "$OMEGA_LOCK_VM"
take_lock "$OMEGA_LOCK_RP"
LOCKS_TAKEN_BY_US=1
install_lock_traps
trap omega_cleanup EXIT
: > "$RUN/pids"
{
  echo "OMEGA-START session=$SESSION rehearsal=$REHEARSAL launch=$OMEGA_LAUNCH hard=$HARD est=${EST}s rows=$ROWS_N"
  echo "uname $(uname -r)"; lscpu 2>/dev/null | head -8
} | tee -a "$RUN/session.log"
$PY -c '
import json, sys
a = sys.argv
pins = json.load(open(a[8])) if a[8] else {}
h = {"kind": "header", "session": a[1], "rehearsal": a[2] == "1", "launch": int(a[3]), "hard": int(a[4]), "est_s": int(a[5]), "rows": int(a[6]),
     "kernel": a[7], "r_run": a[9] or None}
for k in ("d_ms", "sigma_R_ms", "skew_ms", "L", "seed_ok"):
    if k in pins: h[k] = pins[k]
print(json.dumps(h))
' "$SESSION" "$REHEARSAL" "$OMEGA_LAUNCH" "$HARD" "$EST" "$ROWS_N" "$(uname -r)" "$PINS" "${OMEGA_R_RUN:-}" >> "$LEDGER"

# ── 4. the host state, the sysctl pre-flight, the topology, the stack ──
hoststate_snapshot "$RUN/hoststate.pre"
sysctl_preflight || omega_abort ABORT-SYSCTL "pre-flight failed (see above)"
TOPO_STARTED=1
topo_up || omega_abort ABORT-TOOLS "the topology did not come up (ip / tc / ethtool refused)"
sysctl_apply $OMEGA_NS_CLI || omega_abort ABORT-SYSCTL "stack D in $OMEGA_NS_CLI"
sysctl_apply $OMEGA_NS_SRV || omega_abort ABORT-SYSCTL "stack D in $OMEGA_NS_SRV"
hoststate_check "$RUN/hoststate.pre" || omega_abort ABORT-HOSTSTATE "changed right after the first per-namespace write"

# ── 5. the detached stopper: asks for a stop at a round boundary at OMEGA_STOP_AT; kills only the driver by PID, at HARD + 120 s ──
STOP_AT="${OMEGA_STOP_AT:-$(( OMEGA_LAUNCH + 17640 ))}"; [ "$STOP_AT" -le "$HARD" ] || STOP_AT="$HARD"
omega_stopper() { # at hard driver_pid
  while [ "$(date +%s)" -lt "$1" ]; do kill -0 "$3" 2>/dev/null || return 0; omega_sleep 1; done
  touch "$RUN/STOP"
  while [ "$(date +%s)" -lt $(( $2 + 120 )) ]; do kill -0 "$3" 2>/dev/null || return 0; omega_sleep 1; done
  kill -TERM "$3" 2>/dev/null
}
omega_stopper "$STOP_AT" "$HARD" "$$" & omega_record_pid $!

# ── 6. R0 and the tool checks (session R) ──
omega_r0() {
  local g mode dt k e tab ent L pr n r0=$RUN/r0 ok=1 sw t0 seed1 seed2 share names
  mkdir -p "$r0"; : > "$RUN/R0.log"
  PROBE="${OMEGA_PROBE:-$HERE/omega_probe.py}"
  # the table's size L: the largest of 1024 .. 65536 that netem loads
  L=0
  for n in 1024 2048 4096 8192 16384 32768 65536; do
    omega_table "tl$n" 8 "$n"
    q_reorder $OMEGA_NS_MID mid1 "$RUN/tables" "tl$n" REL 1 1000 - >/dev/null 2>&1 && L=$n || break
  done
  echo "R0-TOOL table-size L=$L" | tee -a "$RUN/R0.log"
  [ "$L" -ge 1024 ] || omega_abort ABORT-TOOLS "tc loads no netem distribution table (TC_LIB_DIR=$RUN/tables)"
  omega_table zero 0 "$L"
  OMEGA_L="$L"
  # seed support
  if q_reorder $OMEGA_NS_MID mid1 "$RUN/tables" zero REL 1 1000 42 >/dev/null 2>&1; then OMEGA_SEED_OK=1; else OMEGA_SEED_OK=0; fi
  echo "R0-TOOL netem-seed accepted=$OMEGA_SEED_OK (0: rep halves stand in for seed halves)" | tee -a "$RUN/R0.log"
  # plug and the IP-id selector (the outage check)
  if q_outage_setup >/dev/null 2>&1; then PLUG=1; else PLUG=0; echo "SKIPPED-PLUG the outage check is skipped" | tee -a "$RUN/R0.log"; fi
  nsx $OMEGA_NS_MID tc qdisc replace dev mid1 root netem limit 100000 delay 0us >/dev/null 2>&1
  omega_bg $OMEGA_NS_SRV "$r0/ipid.srv" iperf3 -s -1 -p 5401
  omega_cap $OMEGA_NS_MID mid0 "$r0/ipid.pcap"; e="$OMEGA_LASTPID"
  omega_sleep "${OMEGA_SETTLE_S:-0.5}"
  nsx $OMEGA_NS_CLI iperf3 -c 10.77.2.2 -p 5401 -t 2 > "$r0/ipid.cli" 2>&1
  omega_stop_pid "$e"
  nsx $OMEGA_NS_MID tcpdump -nn -tt -v -r "$r0/ipid.pcap" > "$r0/ipid.txt" 2>/dev/null
  share="$($PY "$HERE/omega_parse.py" ipid "$r0/ipid.txt")"
  if [ "$(awk -v s="$share" 'BEGIN{print (s >= 0.9) ? 1 : 0}')" = 1 ] && [ "$PLUG" = 1 ]; then SKIP_OUT=0; else SKIP_OUT=1; echo "SKIPPED-SELECTOR consecutive-id share=$share: the outage check is skipped" | tee -a "$RUN/R0.log"; fi
  # counter names (7.1): a missing one is printed `-`
  names=""
  for n in TCPDSACKRecv TCPDSACKOldSent TCPSackRecovery TCPTimeouts TCPFullUndo TCPPartialUndo TCPDSACKUndo TCPLossUndo TCPSACKReorder TCPSpuriousRTOs TCPLossProbes; do
    nsx $OMEGA_NS_CLI cat /proc/net/netstat 2>/dev/null | grep -q "$n" && names="$names $n" || names="$names -"
  done
  echo "R0-TOOL counters:$names" | tee -a "$RUN/R0.log"
  # the reorderer alone: a paced probe at 1000 and 10000 /s, per mode and dt, with the table switched mid-run
  for g in 1000 10000; do
    for mode in CLEAN REL HOLD; do
      for k in 3 8; do
        dt="$(awk -v g="$g" -v k="$k" 'BEGIN{printf "%.4f", (k + 0.5) / g * 1000}')"
        pr="$(awk -v g="$g" 'BEGIN{print 1 / (g * 0.1)}')"
        ent="$(awk -v p="$pr" -v L="$L" 'BEGIN{n = int(p * L + 0.5); print (n < 1) ? 1 : n}')"
        omega_table "r0e" "$ent" "$L"
        q_reorder $OMEGA_NS_MID mid1 "$RUN/tables" zero REL 0 "$g" - || omega_abort ABORT-TOOLS "tc refused the zero table"
        tab="$r0/$g-$mode-$k"
        omega_cap $OMEGA_NS_SRV srv0 "$tab.pcap"; e="$OMEGA_LASTPID"
        omega_sleep "${OMEGA_SETTLE_S:-0.5}"
        omega_bg $OMEGA_NS_MID "$tab.out" "${OMEGA_PYTHON:-python3}" -I "$PROBE" 10.77.2.2 "$g" 2 "$tab.send"; pr_pid="$OMEGA_LASTPID"
        omega_sleep 0.8
        sw="$(omega_now)"
        [ "$mode" = CLEAN ] || q_reorder_change $OMEGA_NS_MID mid1 "$RUN/tables" r0e "$mode" "$dt" "$g" - || omega_abort ABORT-TOOLS "tc refused the arm's table"
        wait "$pr_pid" 2>/dev/null
        omega_sleep "${OMEGA_SETTLE_S:-0.5}"
        omega_stop_pid "$e"
        nsx $OMEGA_NS_SRV tcpdump -nn -tt -v -r "$tab.pcap" > "$tab.txt" 2>/dev/null
        r="$($PY "$HERE/omega_parse.py" r0 "$mode" "$dt" "$g" "$k" "$tab.send" "$tab.txt" "$( [ "$mode" = CLEAN ] && echo - || echo "$pr")" "$sw")" || ok=0
        echo "R0 g=$g mode=$mode dt=$dt: $r" | tee -a "$RUN/R0.log"
      done
    done
  done
  [ "$ok" = 1 ] || omega_abort ABORT-BENCH "R0 refutes a reorderer property (see $RUN/R0.log); no battery is launched"
  $PY -c 'import json,sys; print(json.dumps({"L": int(sys.argv[1]), "seed_ok": sys.argv[2] == "1", "skip_outage": sys.argv[3] == "1"}))' "$L" "$OMEGA_SEED_OK" "${SKIP_OUT:-0}" > "$RUN/R0.json"
  echo "R0-PASS" | tee -a "$RUN/R0.log"
}

ROWSF="$RUN/rows.txt"
OMEGA_SEED_OK="${OMEGA_SEED_OK:-1}"
if [ "$SESSION" = R ] && [ "$REHEARSAL" != 1 ]; then
  omega_r0
  omega_tunnel_measure || true
  topo_up || omega_abort ABORT-TOOLS "the topology did not come back after the tunnel measurement"
  sysctl_apply $OMEGA_NS_CLI >/dev/null && sysctl_apply $OMEGA_NS_SRV >/dev/null || omega_abort ABORT-SYSCTL "stack D after the tunnel measurement"
  hoststate_check "$RUN/hoststate.pre" || omega_abort ABORT-HOSTSTATE "after the tunnel measurement"
  QTPINS="$RUN/pins-from-R.json"
  $PY -c '
import json, sys
q = {}
try: q = json.load(open(sys.argv[1]))
except Exception: pass
print(json.dumps({"B_t_pkts": q.get("B_t_pkts", {}) } if q.get("B_t_pkts") else {}))' "$RUN/QT.json" > "$QTPINS"
  PINS="$QTPINS"
fi
$PY omega_cells.py rows "$SESSION_DESIGN" $PINS > "$ROWSF" || omega_abort ABORT-BUILD "cannot list the rows of $SESSION_DESIGN"
if [ "${SKIP_OUT:-0}" = 1 ]; then awk '$5 != "OUT"' "$ROWSF" > "$ROWSF.x" && mv "$ROWSF.x" "$ROWSF"; fi
export OMEGA_L OMEGA_SEED_OK

# stage T: the tunnel comes up when the first T row is reached (omega_run_rows calls this)
omega_stage_t_start() {
  [ -n "${OMEGA_BIN:-}" ] || omega_abort ABORT-BUILD "stage T needs OMEGA_BIN (raptorpath) pinned by OMEGA_BIN_SHA"
  local sha; sha="$(sha256sum "$OMEGA_BIN" | cut -d' ' -f1)"
  [ -n "${OMEGA_BIN_SHA:-}" ] || omega_abort ABORT-BUILD "stage T needs OMEGA_BIN_SHA (the amendment's pin)"
  [ "$sha" = "$OMEGA_BIN_SHA" ] || omega_abort ABORT-BUILD "raptorpath sha $sha differs from the pin $OMEGA_BIN_SHA"
  omega_note "TUNSHA $sha"
  tunnel_up "$OMEGA_BIN" auto || return 1
  tunnel_app_up || return 1
  sysctl_apply $OMEGA_NS_CLI >/dev/null && sysctl_apply $OMEGA_NS_APP >/dev/null || omega_abort ABORT-SYSCTL "stack D in the stage T namespaces"
}

# ── 7. the rows ──
omega_run_rows "$ROWSF"; RC=$?
case "$RC" in
  0) FINAL="done" ;;
  10) FINAL="truncated"; omega_note "TRUNCATED-$SESSION" ;;
  11) omega_abort ABORT-SMOKE "a smoke witness is missing or a row could not be digested (see $RUN/session.log)" ;;
  *) omega_abort ABORT-TOOLS "row loop ended with $RC" ;;
esac

# ── 8. session R: the amendment draft (nothing in it is scored) ──
if [ "$FINAL" = done ] && [ "$SESSION" = R ] && [ "$REHEARSAL" != 1 ]; then
  {
    echo "AMENDMENT DRAFT (session R $RUN): pins for session D1; nothing here is a result"
    echo "R0.json: $(cat "$RUN/R0.json" 2>/dev/null)"
    echo "QT.json (standing queue through the tunnel): $(cat "$RUN/QT.json" 2>/dev/null || echo 'not measured')"
    echo "tunnel qlen / default_qdisc: see session.log (TUNQLEN)"
    echo "calibration (per configuration, n and DE):"
    $PY omega_score.py calibrate "$LEDGER" 2>&1
  } > "$RUN/AMENDMENT.txt"
fi
exit 0
