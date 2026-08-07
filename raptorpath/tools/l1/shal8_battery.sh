#!/bin/bash
# Goal-gate "Ship The Wins 2: shal8 anchor" (2026-08-07) — the pre-registered
# diagnosis pass + fix battery for the shipped default's shallow-buffer
# collapse (`RWM_QUIC_CC=bbr_rs`, the burst-robust BBR).
#
#   usage: sudo bash shal8_battery.sh diag <seed> [reps]     # instrumented pass
#          sudo bash shal8_battery.sh <seed> [reps]          # fix battery
#
# DIAG PASS (pre-registered ×3 reps s42 + 1 spot rep s7, shal8 only):
#   defdiag = env unset + RWM_DIAG=1        (P-D1: qcwnd gauge ≫ true BDP·MTU,
#                                            engine btlbw ~100k sym/s class)
#   plainrs = RWM_PLAIN_RS=1 + RWM_DIAG=1   (P-D2: honest sampler ≈ 1× link,
#                                            goodput UNCHANGED in the collapse
#                                            class — gauge, not driver)
# FIX BATTERY (seeds 42+7, interleaved round-robin per rep):
#   adv cells (adv_cells.sh, 25 MB × 1): shal8 ×reps def/fix/copa,
#                                        c2ctl ×reps def/fix
#   topo cells (perf_rwm_c.sh):          c1 single 400 MB ×4 def/fix,
#                                        sc2 single 100 MB ×4 def/fix,
#                                        c7 dual 200 MB ×4 def/fix
#   crown (tail_matrix c2, ×4 reps):     default + bbrrs arms, {400,1200} B
# Arms: def = env unset (CURRENT shipped default), fix = RWM_QUIC_CC=bbr_rs,
# copa = RWM_QUIC_CC=passthrough (the class reference).
# CC-liveness echoes asserted per arm; ARMCOUNT loud-fail (discipline 1/7).
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
MODE_ARG="${1:-42}"
if [ "$MODE_ARG" = "diag" ]; then
  DIAG_MODE=1; SEED_ARG="${2:-42}"; REPS="${3:-3}"
else
  DIAG_MODE=0; SEED_ARG="$MODE_ARG"; REPS="${2:-8}"
fi
SPOT_REPS=4
BIN=/home/vibe/raptorpath/target/release/raptorpath
ODIR=/home/vibe/shal8fix
OUT=$ODIR/$([ "$DIAG_MODE" = 1 ] && echo diag || echo battery)-s${SEED_ARG}.log
DDIR=$ODIR/diag
mkdir -p "$DDIR" "$ODIR"
: >> "$OUT"
echo "# shal8fix $([ "$DIAG_MODE" = 1 ] && echo DIAG-PASS || echo BATTERY) $(date -u +%FT%TZ) seed=$SEED_ARG reps=$REPS" >> "$OUT"
echo "# binary: $(sha256sum $BIN)" >> "$OUT"
echo "# source: $(cat /home/vibe/raptorpath/COMMIT 2>/dev/null || echo unknown)" >> "$OUT"
echo "# kernel: $(uname -r)" >> "$OUT"
lscpu | grep "Model name" >> "$OUT"
(lscpu | grep -E "^Flags" | grep -oE "aes|avx2|pclmulqdq" | sort -u | tr '\n' ' ' >> "$OUT") || true
echo >> "$OUT"

BYTES_ADV=25000000
BASEENV="RWM_GEN=0 RWM_DIAG=1 RWM_PERF_TIMEOUT_S=120"

arm_env() { # arm -> extra env
  case "$1" in
    def|defdiag) echo "" ;;
    fix)         echo "RWM_QUIC_CC=bbr_rs" ;;
    copa)        echo "RWM_QUIC_CC=passthrough" ;;
    plainrs)     echo "RWM_PLAIN_RS=1" ;;
  esac
}

check_cc_liveness() { # arm log -> LIVENESS/CONTAMINATION lines (discipline 1)
  local arm="$1" log="$2" bbrn rsn ptn feedn smpn
  bbrn=$(grep -ac "congestion controller: BBR" "$log" 2>/dev/null || true)
  rsn=$(grep -ac "burst-robust BBR" "$log" 2>/dev/null || true)
  ptn=$(grep -ac "quinn congestion window is engine-owned" "$log" 2>/dev/null || true)
  feedn=$(grep -ac "Copa delivery feed ACTIVE" "$log" 2>/dev/null || true)
  smpn=$(grep -ac "send-interval SAMPLER ACTIVE" "$log" 2>/dev/null || true)
  echo "LIVENESS arm=$arm bbr=$bbrn bbr_rs=$rsn pt=$ptn feed=$feedn sampler=$smpn" >> "$OUT"
  case "$arm" in
    def|defdiag)
      [ "$bbrn" -eq 0 ] && echo "ARM-LIVENESS-FAIL arm=$arm rep=$REP (no BBR echo)" >> "$OUT"
      { [ "$rsn" -gt 0 ] || [ "$ptn" -gt 0 ] || [ "$smpn" -gt 0 ]; } && echo "ARM-CONTAMINATION arm=$arm rep=$REP" >> "$OUT" ;;
    plainrs)
      { [ "$bbrn" -eq 0 ] || [ "$smpn" -eq 0 ]; } && echo "ARM-LIVENESS-FAIL arm=$arm rep=$REP (bbr=$bbrn sampler=$smpn)" >> "$OUT"
      { [ "$rsn" -gt 0 ] || [ "$ptn" -gt 0 ]; } && echo "ARM-CONTAMINATION arm=$arm rep=$REP" >> "$OUT" ;;
    fix)
      [ "$rsn" -eq 0 ] && echo "ARM-LIVENESS-FAIL arm=$arm rep=$REP (no bbr_rs echo)" >> "$OUT"
      { [ "$bbrn" -gt 0 ] || [ "$ptn" -gt 0 ]; } && echo "ARM-CONTAMINATION arm=$arm rep=$REP" >> "$OUT" ;;
    copa)
      { [ "$ptn" -eq 0 ] || [ "$feedn" -eq 0 ]; } && echo "ARM-LIVENESS-FAIL arm=$arm rep=$REP (pt=$ptn feed=$feedn)" >> "$OUT"
      { [ "$bbrn" -gt 0 ] || [ "$rsn" -gt 0 ]; } && echo "ARM-CONTAMINATION arm=$arm rep=$REP" >> "$OUT" ;;
  esac
  return 0
}

run_adv() { # cell arm  (adv_cells.sh single-path, 25 MB, ADVRESULT row)
  local cell="$1" arm="$2" name="$1-$2"
  local envs; envs="$(arm_env "$arm")"
  local t0; t0=$(date +%s)
  echo "=== rep=$REP arm=$name seed=$SEED_ARG env=\"$envs\" cell=$cell bytes=$BYTES_ADV $(date -u +%T)" >> "$OUT"
  pkill -x raptorpath 2>/dev/null || true
  rm -f /tmp/s8-c.log /tmp/s8-s.log /tmp/s8-c-clean.log /tmp/s8-q.txt
  if ! bash ./adv_cells.sh up "$cell" --seed "$SEED_ARG" >/dev/null 2>&1; then
    echo "TOPO-PING abort $name rep=$REP — retrying once" >> "$OUT"
    if ! bash ./adv_cells.sh up "$cell" --seed "$SEED_ARG" >/dev/null 2>&1; then
      echo "TOPO-ABORT $name rep=$REP (double abort, invocation dropped)" >> "$OUT"
      bash ./adv_cells.sh down >/dev/null 2>&1 || true
      return 0
    fi
  fi
  # shellcheck disable=SC2086
  ip netns exec rp-srv env $BASEENV $envs "$BIN" perf --server --bind 10.77.0.2:7000 \
      --window-reliable --protocol-hint bulk >/tmp/s8-s.log 2>&1 &
  local up=0
  for _ in $(seq 1 20); do
    ip netns exec rp-srv ss -uln 2>/dev/null | grep -q ':7000' && { up=1; break; }
    sleep 0.3
  done
  if [ "$up" -eq 0 ]; then
    echo "SRV-BIND-FAIL $name rep=$REP (invocation dropped)" >> "$OUT"
    pkill -x raptorpath 2>/dev/null || true
    bash ./adv_cells.sh down >/dev/null 2>&1 || true
    return 0
  fi
  sleep 1
  # shellcheck disable=SC2086
  timeout 300 ip netns exec rp-cli env $BASEENV $envs "$BIN" perf --client \
      --peer 10.77.0.2:7000 --bind 10.77.0.1:0 \
      --window-reliable --protocol-hint bulk \
      --bytes "$BYTES_ADV" --runs 1 >/tmp/s8-c.log 2>&1
  local rc=$?
  [ "$rc" -ne 0 ] && echo "CLIENT-RC $name rep=$REP rc=$rc" >> "$OUT"
  sed 's/\x1b\[[0-9;]*m//g' /tmp/s8-c.log > /tmp/s8-c-clean.log 2>/dev/null || true
  (grep -E '"summary"|"dnf"|"run"' /tmp/s8-c-clean.log | tail -3 >> "$OUT") || true
  bash ./adv_cells.sh counters > /tmp/s8-q.txt 2>/dev/null || true
  echo "RUNTIME $name rep=$REP $(( $(date +%s) - t0 ))s" >> "$OUT"
  check_cc_liveness "$arm" /tmp/s8-c-clean.log

  # Per-run row: goodput + drop truth + the P-D1/P-F1 gauges (qcwnd, btlbw).
  python3 - "$cell" "$arm" "$SEED_ARG" "$REP" <<'EOF' >> "$OUT" 2>&1 || echo "S8RESULT-PARSE-FAIL $name rep=$REP" >> "$OUT"
import json, re, sys
cell, arm, seed, rep = sys.argv[1:5]
mbps = secs = None; dnf = False
try:
    for line in open("/tmp/s8-c-clean.log", errors="replace"):
        line = line.strip()
        if line.startswith("{"):
            try: j = json.loads(line)
            except Exception: continue
            if j.get("run") == 1:
                if j.get("dnf"): dnf = True
                else: mbps, secs = j.get("mbps"), j.get("seconds")
except FileNotFoundError:
    dnf = True

# DIAG gauges (steady state = per-path DIAG lines 4+): engine btlbw, quinn
# qcwnd/qce/qlp, wireQ, pl, retx.
btl, qcw, wq, pls = [], [], [], []
qce = qlost = qsent = retx = 0
nline = 0
rtt_pat = re.compile(r"rtt=(\d+)/wrtt=(\d+)/rtp(\d+)ms")
btl_pat = re.compile(r"btlbw=(\d+)")
qc_pat = re.compile(r"qcwnd=(\d+) qce=(\d+) qlp=(\d+)/(\d+)")
pl_pat = re.compile(r"pl=([0-9.]+)")
rx_pat = re.compile(r"retx=(\d+)")
try:
    for line in open("/tmp/s8-c-clean.log", errors="replace"):
        m = rtt_pat.search(line)
        if m:
            nline += 1
            if nline >= 4:
                rtt, wrtt, rtp = map(int, m.groups())
                if wrtt > 0 and rtp > 0: wq.append(max(wrtt - rtp, 0))
                b = btl_pat.search(line)
                if b: btl.append(int(b.group(1)))
                q = qc_pat.search(line)
                if q:
                    qcw.append(int(q.group(1)))
                    qce = max(qce, int(q.group(2)))
                    qlost = max(qlost, int(q.group(3)))
                    qsent = max(qsent, int(q.group(4)))
                p = pl_pat.search(line)
                if p: pls.append(float(p.group(1)))
        r = rx_pat.search(line)
        if r: retx = max(retx, int(r.group(1)))
except FileNotFoundError:
    pass

# Cell-truth (tc -s): bottleneck sent/dropped per the adv_battery parser.
sec = None; stats = {}; kind = None
sent_re = re.compile(r"Sent (\d+) bytes (\d+) pkts? \(dropped (\d+)")
try:
    for line in open("/tmp/s8-q.txt", errors="replace"):
        if line.startswith("== "):
            sec = line[3:].split()[0]; kind = None; continue
        mq = re.match(r"\s*qdisc (\w+)", line)
        if mq: kind = mq.group(1)
        if re.match(r"\s*police ", line): kind = "police"
        ms = sent_re.search(line)
        if ms and sec and kind:
            k = (sec, kind); s0, d0 = stats.get(k, (0, 0))
            stats[k] = (s0 + int(ms.group(2)), d0 + int(ms.group(3)))
except FileNotFoundError:
    pass
bn = stats.get(("CLI0", "netem"))
bn_sent, bn_drop = (bn or (None, None))
frac = None
if bn_sent is not None and (bn_sent + bn_drop) > 0:
    frac = bn_drop / (bn_sent + bn_drop)
tbf_drop = stats.get(("CLI0", "tbf"), (None, None))[1] if cell == "shal8" else None

def med(v):
    if not v: return None
    v = sorted(v); return v[len(v)//2]

print("S8RESULT " + json.dumps({
    "cell": cell, "arm": arm, "seed": int(seed), "rep": int(rep),
    "dnf": dnf, "mbps": mbps, "seconds": secs,
    "btlbw_med": med(btl), "qcwnd_med": med(qcw), "qcwnd_max": max(qcw) if qcw else None,
    "qce": qce, "qlost": qlost, "qsent": qsent,
    "wq_p50": med(wq), "pl_max": max(pls) if pls else None, "retx": retx,
    "bn_sent_pkt": bn_sent, "bn_drop": bn_drop,
    "bn_drop_frac": round(frac, 4) if frac is not None else None,
    "tbf_drop": tbf_drop,
}))
EOF

  cp /tmp/s8-c.log "$DDIR/${name}-s${SEED_ARG}-r${REP}-c.log" 2>/dev/null || true
  cp /tmp/s8-s.log "$DDIR/${name}-s${SEED_ARG}-r${REP}-s.log" 2>/dev/null || true
  cp /tmp/s8-q.txt "$DDIR/${name}-s${SEED_ARG}-r${REP}-q.txt" 2>/dev/null || true
  pkill -x raptorpath 2>/dev/null || true
  bash ./adv_cells.sh down >/dev/null 2>&1 || true
}

run_topo() { # name envs cellA cellB bytes mode  (perf_rwm_c.sh spot cells)
  local name="$1" envs="$2" ca="$3" cb="$4" bytes="$5" mode="$6" attempt ok=0
  for attempt in 1 2 3; do
    for _ in $(seq 1 30); do
      if ! pgrep -x raptorpath >/dev/null 2>&1 \
         && ! ip netns exec rp-srv ss -uln 2>/dev/null | grep -q ':7000'; then
        break
      fi
      pkill -x raptorpath 2>/dev/null || true
      sleep 1
    done
    rm -f /tmp/rwm-c.log /tmp/rwm-s.log
    echo "=== rep=$REP arm=$name attempt=$attempt seed=$SEED_ARG env=\"$envs\" cell=$ca/$cb/$mode bytes=$bytes $(date -u +%T)" >> "$OUT"
    # shellcheck disable=SC2086
    env SEED=$SEED_ARG RWM_GEN=0 $envs RWM_DIAG=1 \
      bash perf_rwm_c.sh "$ca" "$cb" bulk "$bytes" 1 "$mode" 2>&1 \
      | grep -E "summary|\"dnf\"|CPU:|QDISC cli" >> "$OUT" || true
    { sed 's/\x1b\[[0-9;]*m//g' /tmp/rwm-c.log > /tmp/rwm-c-clean.log; } 2>/dev/null || true
    check_cc_liveness "${name##*-}" /tmp/rwm-c-clean.log
    if grep -a '"summary":true' /tmp/rwm-c.log >/dev/null 2>&1; then
      ok=1
      cp /tmp/rwm-c.log "$DDIR/bat-${name}-s${SEED_ARG}-r${REP}-c.log" 2>/dev/null || true
      break
    fi
    echo "RUN-RETRY rep=$REP arm=$name attempt=$attempt (no summary)" >> "$OUT"
    cp /tmp/rwm-c.log "$DDIR/bat-${name}-s${SEED_ARG}-r${REP}-a${attempt}-FAILED-c.log" 2>/dev/null || true
  done
  [ $ok = 1 ] || echo "RUN-LOST rep=$REP arm=$name after 3 attempts" >> "$OUT"
}

if pgrep -x raptorpath >/dev/null 2>&1; then
  echo "BUSY: raptorpath already running -- aborting" | tee -a "$OUT" >&2
  exit 3
fi

if [ "$DIAG_MODE" = 1 ]; then
  # ── Instrumented diagnosis pass (P-D1/P-D2), shal8 only ────────────────
  for REP in $(seq 1 "$REPS"); do
    run_adv shal8 defdiag
    run_adv shal8 plainrs
  done
  for a in shal8-defdiag shal8-plainrs; do
    res=$(grep -c "\"cell\": \"${a%-*}\", \"arm\": \"${a##*-}\"" "$OUT" || true)
    echo "ARMCOUNT $a results=$res" >> "$OUT"
    [ "$res" -eq 0 ] && echo "ARM-VANISHED $a" >> "$OUT"
  done
  echo "DIAG-DONE seed=$SEED_ARG $(date -u +%FT%TZ)" >> "$OUT"
  echo DIAG-DONE-$SEED_ARG
  exit 0
fi

# ── Fix battery ──────────────────────────────────────────────────────────
E_FIX="RWM_QUIC_CC=bbr_rs"
for REP in $(seq 1 "$REPS"); do
  run_adv shal8 def; run_adv shal8 fix; run_adv shal8 copa
  run_adv c2ctl def; run_adv c2ctl fix
  if [ "$REP" -le "$SPOT_REPS" ]; then
    run_topo c1-def  ""       c1 c1 400000000 single
    run_topo c1-fix  "$E_FIX" c1 c1 400000000 single
    run_topo sc2-def ""       c2 c2 100000000 single
    run_topo sc2-fix "$E_FIX" c2 c2 100000000 single
    run_topo c7-def  ""       c2 c2 200000000 dual
    run_topo c7-fix  "$E_FIX" c2 c2 200000000 dual
  fi
done

# Crown rows (tail_matrix, default machine vs the fix arm, c2 clean).
echo "=== TAILROW c2 default+bbrrs $(date -u +%T)" >> "$OUT"
SEED=$SEED_ARG RWM_TM_ARMS='default bbrrs' bash ./tail_matrix.sh c2 4 >> "$OUT" 2>&1 || echo "TAILROW-FAIL c2" >> "$OUT"

# ARMCOUNT (discipline 7): loud zero-row failure.
for a in shal8-def shal8-fix shal8-copa c2ctl-def c2ctl-fix; do
  res=$(grep -c "\"cell\": \"${a%-*}\", \"arm\": \"${a##*-}\"" "$OUT" || true)
  echo "ARMCOUNT $a results=$res" >> "$OUT"
  [ "$res" -eq 0 ] && echo "ARM-VANISHED $a" >> "$OUT"
done
for a in c1-def c1-fix sc2-def sc2-fix c7-def c7-fix; do
  n=$(awk "/=== rep=.* arm=$a /{f=1} f&&/\"summary\":true/{c++;f=0} END{print c+0}" "$OUT")
  echo "ARMCOUNT $a n=$n" >> "$OUT"
  [ "$n" -eq 0 ] && echo "ARM-VANISHED $a" >> "$OUT"
done
echo "BATTERY-DONE seed=$SEED_ARG $(date -u +%FT%TZ)" >> "$OUT"
echo BATTERY-DONE-$SEED_ARG
