#!/bin/bash
# One row (one rep of one arm of one configuration) of the disorder-price (omega) measurement, and the loop over a session's rows.
# SOURCED by omega_run_all.sh after omega_topo.sh. It defines functions only. Registration sections 4.4, 7.1, 7.2.
#
# A row: reset the qdiscs of its configuration -> witness before -> pcaps, backlog sampler (every 10 ms), iperf3 -P 8 -> warm-up until the
# 1 s median backlog reaches 0.75 B (NO_FILL at 60 s; no event before it) -> the arm's table goes live -> the event phase -> stop everything
# by recorded PID -> witness after -> digest (omega_parse.py) -> one ledger line. Process control is by PID only (no pkill -f, no killall).
#
# Variables from the caller: RUN LEDGER HERE PY PINS (file or empty) SESSION HARD TOPO_SH; the path (omega_path_netem / omega_path_tunnel) sets
# PRE_NS PRE_DEV POST_NS POST_DEV RO_NS RO_DEV CLI_NS SRV_NS SRV_ADDR.
# Time seams (defaults are the registered values; the stub run shrinks them): OMEGA_WIN_S 1.0 OMEGA_NOFILL_S 60 OMEGA_EVENT_S 12 OMEGA_SAMPLE_S 0.01
# OMEGA_POLL_S 0.25 OMEGA_SETTLE_S 0.5.

omega_path_netem() {
  PRE_NS=$OMEGA_NS_MID; PRE_DEV=mid0; POST_NS=$OMEGA_NS_SRV; POST_DEV=srv0; RO_NS=$OMEGA_NS_MID; RO_DEV=mid1
  CLI_NS=$OMEGA_NS_CLI; SRV_NS=$OMEGA_NS_SRV; SRV_ADDR=10.77.2.2
}
omega_path_tunnel() {   # stage T: the real tunnel in the path; the reorderer sits behind rp-srv, after TUN delivery
  PRE_NS=$OMEGA_NS_SRV; PRE_DEV=rpsrv0; POST_NS=$OMEGA_NS_APP; POST_DEV=app0; RO_NS=$OMEGA_NS_SRV; RO_DEV=srv1
  CLI_NS=$OMEGA_NS_CLI; SRV_NS=$OMEGA_NS_APP; SRV_ADDR=10.88.0.2
}

omega_now() { date +%s.%N; }
omega_sleep() { "${OMEGA_SLEEP:-sleep}" "$1"; }
omega_note() { echo "$*" | tee -a "$RUN/session.log"; }

omega_record_pid() { echo "$1" >> "$RUN/pids"; }
omega_stop_pid() { # pid  -- TERM by recorded PID, then reap
  local p="$1"
  [ -n "$p" ] || return 0
  kill -TERM "$p" 2>/dev/null || true
  wait "$p" 2>/dev/null || true
}

omega_cap() { # ns dev pcap   start tcpdump, headers only; its PID is in OMEGA_LASTPID
  omega_bg "$1" "$3.log" tcpdump -i "$2" -nn -s 96 -w "$3"
}

omega_table() { # name delayed_entries size  -> $RUN/tables/<name>.dist
  $PY "$HERE/omega_dist.py" make "$RUN/tables" "$1" "$2" "$3"
}

# Prepare one configuration (qdiscs that do not change between its rows)
omega_config_prepare() { # cfg
  local cfg="$1" P g rtt limit
  P="$($PY "$HERE/omega_cells.py" param "$cfg" CLEAN $PINS)" || return 1
  read -r g rtt limit <<< "$($PY -c 'import json,sys; p=json.loads(sys.argv[1]); print(p["g"], p["rtt"], p["limit"])' "$P")"
  q_bottleneck "$g" "$limit" && q_rtt "$rtt"
}

# One row. Returns 0 (row written), 10 (truncation requested: no row), 11 (ABORT-SMOKE).
omega_row() { # cfg arm rep half stage
  local cfg="$1" arm="$2" rep="$3" half="$4" stage="$5" rid rdir P g rtt limit B tau kind dt ent sp label L est seed
  local mpid="" spid="" ppid="" cpid="" srvpid="" ti="" tw="" nofill=0 t_event t_launch row meta wj sysok=true warm_ok=false okc t_out
  rid="$(printf '%s.%s.r%s.%s' "$cfg" "$arm" "$rep" "$stage" | tr -c 'A-Za-z0-9._\n-' '_')"
  rdir="$RUN/rows/$rid"; mkdir -p "$rdir"
  P="$($PY "$HERE/omega_cells.py" param "$cfg" "$arm" $PINS)" || { omega_note "ABORT-BUILD no such row $cfg $arm"; return 11; }
  echo "$P" > "$rdir/params.json"
  read -r g rtt limit B tau kind dt ent sp label L <<< "$($PY -c 'import json,sys; p=json.loads(sys.argv[1]); print(p["g"], p["rtt"], p["limit"], p["B"], p["tau_q_ms"], p["kind"], p["dt_ms"], p["entries"], p["spacing_s"], p["label"], p["L"])' "$P")"
  est=$([ "${cfg##*-}" = Qs ] && echo 19 || echo 41); est="${OMEGA_ROW_EST_S:-$est}"
  # the hard deadline (an LD_HARD-style epoch): a row that cannot finish before it is not started
  if [ "$(awk -v n="$(date +%s)" -v e="$est" -v h="$HARD" 'BEGIN{print (n + e > h) ? 1 : 0}')" = 1 ]; then
    omega_note "TRUNCATE-HARD $rid would end after OMEGA_HARD=$HARD"; return 10
  fi
  L="${OMEGA_L:-$L}"
  seed="-"; [ "${OMEGA_SEED_OK:-1}" != 1 ] || { [ "$half" = 1 ] && seed=42 || seed=7; }
  if [ "$kind" != CLEAN ] && [ "$kind" != OUT-REL ] && [ "$kind" != OUT-HOLD ] && [ "$ent" = None ]; then
    omega_note "SKIPPED-TABLE $rid (p below 1/L at L=$L)"; echo "{\"kind\":\"row\",\"cfg\":\"$cfg\",\"arm\":\"$arm\",\"armkind\":\"$kind\",\"label\":\"$label\",\"rep\":$rep,\"stage\":\"$stage\",\"status\":\"SKIPPED-TABLE\",\"witness\":{}}" >> "$LEDGER"; return 0
  fi
  # qdiscs of the row: bottleneck, RTT, and the reorderer at its zero table (every arm starts at zero and switches at the witness)
  omega_table zero 0 "$L"
  case "$kind" in REL|HOLD) omega_table "e$ent" "$ent" "$L" ;; esac
  if [ "$cfg" != T ]; then q_bottleneck "$g" "$limit" && q_rtt "$rtt" || { omega_note "ABORT-TOOLS tc refused the bottleneck / RTT qdisc"; return 11; }; fi
  case "$kind" in
    OUT-REL|OUT-HOLD) q_outage_setup || {
      omega_note "SKIPPED-PLUG $rid"
      echo "{\"kind\":\"row\",\"cfg\":\"$cfg\",\"arm\":\"$arm\",\"armkind\":\"$kind\",\"label\":\"$label\",\"rep\":$rep,\"stage\":\"$stage\",\"status\":\"SKIPPED-PLUG\",\"witness\":{}}" >> "$LEDGER"; return 0; } ;;
    OUT*) ;;
    *) q_reorder "$RO_NS" "$RO_DEV" "$RUN/tables" zero REL 0 "$g" "$seed" || { omega_note "ABORT-TOOLS tc refused the reorderer qdisc (table load?)"; return 11; } ;;
  esac
  tcp_metrics_flush "$CLI_NS" "$SRV_NS"
  nsx $OMEGA_NS_CLI tc -s qdisc show dev cli0 > "$rdir/tc-before-cli0.txt" 2>/dev/null
  nsx "$RO_NS" tc -s qdisc show dev "$RO_DEV" > "$rdir/tc-before-ro.txt" 2>/dev/null
  # captures (pre = the bottleneck's output, post = after the reorderer), the sampler and iperf3
  omega_cap "$PRE_NS" "$PRE_DEV" "$rdir/pre.pcap"; mpid="$OMEGA_LASTPID"
  omega_cap "$POST_NS" "$POST_DEV" "$rdir/post.pcap"; ppid="$OMEGA_LASTPID"
  : > "$rdir/backlog.txt"
  bash "$HERE/omega_sampler.sh" "$rdir/backlog.txt" 2> "$rdir/sampler.err" & spid=$!; omega_record_pid "$spid"
  omega_bg "$SRV_NS" "$rdir/iperf-s.log" iperf3 -s -1 -p 5201 --pidfile "$rdir/iperf-s.pid"; srvpid="$OMEGA_LASTPID"
  omega_sleep "${OMEGA_SETTLE_S:-0.5}"
  t_launch="$(omega_now)"
  omega_bg "$CLI_NS" "$rdir/iperf.json" iperf3 -c "$SRV_ADDR" -p 5201 -P 8 -J --get-server-output -i 1 -t "$(awk -v a="${OMEGA_NOFILL_S:-60}" -v b="${OMEGA_EVENT_S:-12}" 'BEGIN{printf "%d", a + b + 5.999}')"; cpid="$OMEGA_LASTPID"
  # warm-up ends on the witness, not on a clock; no event is injected before it
  while :; do
    ti="$($PY "$HERE/omega_parse.py" warmup "$rdir/backlog.txt" "$B" "${OMEGA_WIN_S:-1.0}" "${OMEGA_NOFILL_S:-60}" "$t_launch")"
    case "$ti" in WITNESS*) tw="${ti#WITNESS }"; warm_ok=true; break ;; esac
    if [ "$(awk -v n="$(omega_now)" -v l="$t_launch" -v m="${OMEGA_NOFILL_S:-60}" 'BEGIN{print (n - l > m) ? 1 : 0}')" = 1 ]; then nofill=1; break; fi
    omega_sleep "${OMEGA_POLL_S:-0.25}"
  done
  t_event="$(omega_now)"
  echo "$t_event" > "$rdir/t_event"
  if [ "$nofill" = 0 ]; then
    case "$kind" in
      CLEAN|OUT-NONE) ;;
      OUT-REL) t_out="$(omega_now)"; q_outage REL 1 ;;
      OUT-HOLD) t_out="$(omega_now)"; q_outage HOLD 1 ;;
      *) q_reorder_change "$RO_NS" "$RO_DEV" "$RUN/tables" "e$ent" "$kind" "$dt" "$g" "$seed" || omega_note "ABORT-TOOLS tc refused the arm's table at the witness" ;;
    esac
    omega_sleep "${OMEGA_EVENT_S:-12}"
  fi
  sysctl_readback_ok "$CLI_NS" && sysctl_readback_ok "$SRV_NS" || sysok=false
  nsx $OMEGA_NS_CLI tc -s qdisc show dev cli0 > "$rdir/tc-after-cli0.txt" 2>/dev/null
  nsx "$RO_NS" tc -s qdisc show dev "$RO_DEV" > "$rdir/tc-after-ro.txt" 2>/dev/null
  # stop by recorded PID: iperf3 gets TERM (a background job ignores SIGINT; iperf3 handles TERM like INT and prints the interval JSON), the rest TERM too
  kill -TERM "$cpid" 2>/dev/null || true; wait "$cpid" 2>/dev/null || true
  omega_stop_pid "$srvpid"; omega_stop_pid "$mpid"; omega_stop_pid "$ppid"; omega_stop_pid "$spid"
  if [ "$nofill" = 1 ] && [ ! -s "$rdir/backlog.txt" ]; then
    # no backlog readout at all: the witness is missing, which is not a NO_FILL (a queue that never filled)
    [ "$stage" != smoke ] || { omega_note "ABORT-SMOKE $rid: the backlog sampler read nothing (tc counters absent)"; return 11; }
    echo "{\"kind\":\"row\",\"cfg\":\"$cfg\",\"arm\":\"$arm\",\"armkind\":\"$kind\",\"label\":\"$label\",\"rep\":$rep,\"seedhalf\":$half,\"stage\":\"$stage\",\"session\":\"$SESSION\",\"status\":\"VOID-WITNESS\",\"witness\":{}}" >> "$LEDGER"
    omega_note "VOID-WITNESS $rid (no backlog readout)"; rm -f "$rdir"/*.pcap; return 0
  fi
  if [ "$nofill" = 1 ]; then
    echo "{\"kind\":\"row\",\"cfg\":\"$cfg\",\"arm\":\"$arm\",\"armkind\":\"$kind\",\"label\":\"$label\",\"rep\":$rep,\"seedhalf\":$half,\"stage\":\"$stage\",\"session\":\"$SESSION\",\"status\":\"NO_FILL\",\"witness\":{\"warm_ok\":false}}" >> "$LEDGER"
    omega_note "NO_FILL $rid samples=$(wc -l < "$rdir/backlog.txt") B=$B"; rm -f "$rdir"/*.pcap; return 0
  fi
  # digest
  nsx "$PRE_NS" tcpdump -nn -tt -v -r "$rdir/pre.pcap" > "$rdir/pre.txt" 2>/dev/null
  nsx "$POST_NS" tcpdump -nn -tt -v -r "$rdir/post.pcap" > "$rdir/post.txt" 2>/dev/null
  wj="$($PY "$HERE/omega_parse.py" witness "$kind" "$g" "$([ "$cfg" = T ] && echo 0 || echo "$limit")" "$rdir/tc-before-ro.txt" "$rdir/tc-after-ro.txt" "$rdir/tc-before-cli0.txt" "$rdir/tc-after-cli0.txt")"
  meta="$rdir/meta.json"
  $PY -c '
import json, sys
a = sys.argv
w = json.loads(a[1]); w["sysctl_ok"] = a[2] == "true"; w["warm_ok"] = a[3] == "true"
m = dict(cfg=a[4], arm=a[5], kind=a[6], label=a[7], dt_ms=float(a[8]), rep=int(a[9]), seedhalf=int(a[10]), stage=a[11], session=a[12], g=float(a[13]), rtt=float(a[14]),
         q=a[4].rsplit("-", 1)[-1], limit=int(a[15]), B=float(a[16]), tau_q_ms=float(a[17]), t_event=float(a[18]), event_s=float(a[19]), witness=w, rehearsal=a[20] == "1")
if a[21]: m["t_outage"] = float(a[21])
if m["kind"] == "CLEAN":
    p = json.loads(a[22]); m["pseudo_dts"] = p
if m["stage"] == "OUT" and m["kind"] == "CLEAN": m["kind"] = "OUT-NONE"
json.dump(m, open(a[23], "w"))
' "$wj" "$sysok" "$warm_ok" "$cfg" "$arm" "$kind" "$label" "$dt" "$rep" "$half" "$stage" "$SESSION" "$g" "$rtt" "$limit" "$B" "$tau" "$t_event" "${OMEGA_EVENT_S:-12}" "${REHEARSAL:-0}" "${t_out:-}" "$($PY "$HERE/omega_cells.py" pseudo "$cfg" $PINS)" "$meta"
  row="$($PY "$HERE/omega_parse.py" digest "$meta" "$rdir/pre.txt" "$rdir/post.txt" "$rdir/backlog.txt" "$rdir/iperf.json")" || { omega_note "ABORT-SMOKE digest failed for $rid"; return 11; }
  echo "$row" >> "$LEDGER"
  okc="$(echo "$row" | $PY -c 'import json,sys; r=json.load(sys.stdin); print(len(r.get("witness_missing", [])), r.get("status"))')"
  case "$stage" in smoke) [ "${okc%% *}" = 0 ] || { omega_note "ABORT-SMOKE a witness is missing: $rid: $(echo "$row" | $PY -c 'import json,sys; print(json.load(sys.stdin).get("witness_missing"))')"; return 11; } ;; esac
  # raw pcaps: only rep 1 of each cell is kept
  [ "$rep" = 1 ] || rm -f "$rdir"/*.pcap
  omega_note "ROW $rid status=${okc#* }"
  return 0
}

# The loop over a session's rows (a file of "cfg arm rep half stage" lines). Honours the stopper (balanced round boundary) and the hard deadline.
omega_run_rows() { # rowsfile
  local cfg arm rep half stage curcfg="" currep="" rc nrows=0
  while read -r cfg arm rep half stage; do
    [ -n "$cfg" ] || continue
    if [ "$cfg:$rep" != "$curcfg:$currep" ]; then
      # a round boundary: honour a stop request here (balanced rounds)
      if [ -e "$RUN/STOP" ] && [ "$nrows" -gt 0 ]; then omega_note "TRUNCATE-STOP the stopper fired; ending at a round boundary"; return 10; fi
      curcfg="$cfg"; currep="$rep"
    fi
    if [ "$cfg" = T ]; then
      [ -z "$TSKIP" ] || continue
      if [ "$TUNNEL_UP" != 1 ]; then
        omega_stage_t_start || { TSKIP=1; omega_note "TUN-BRINGUP-FAIL stage T is skipped, not the session"; continue; }
        TUNNEL_UP=1; LASTCFG=T
      fi
      omega_path_tunnel
    else
      omega_path_netem
    fi
    if [ "$cfg" != "${LASTCFG:-}" ]; then
      [ "$cfg" = T ] || omega_config_prepare "$cfg" || { omega_note "ABORT-TOOLS configuration $cfg"; return 11; }
      LASTCFG="$cfg"
    fi
    omega_row "$cfg" "$arm" "$rep" "$half" "$stage"; rc=$?
    [ "$rc" = 0 ] || return "$rc"
    nrows=$((nrows + 1))
  done < "$1"
  omega_note "ROWS-DONE $nrows"
  return 0
}
