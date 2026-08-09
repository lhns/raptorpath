#!/bin/bash
# ARC B1 adversarial-cells battery (goal-gate "Adversarial Cells (B1)" —
# the ADR-0068 prerequisite: MEASURE the pre-registered Copa breakage on
# the cells the clean rig cannot express, with BBR-under as the comparison
# arm on every cell).
#
# Cells (tools/l1/adv_cells.sh): c2ctl (clean control) · jit0/jit5/jit15/
# jit25 (delay-jitter dose-response) · shal8 (8-pkt bottleneck buffer) ·
# pol100 (token-bucket policer, drop-without-queue).
# Arms (all PLAIN RWM_GEN=0, RWM_DIAG=1, full shipped default stack):
#   bbr     = env unset               (BBR-under, the shipped default)
#   copa    = RWM_QUIC_CC=passthrough (Copa-sole: wire + delta(hint) + feed)
#   compete = passthrough + RWM_COPA_COMPETE=1 (pol100/shal8 only — the
#             pre-registered mode-detection arms)
# Interleaved round-robin per rep (discipline 3), fresh tunnel + fresh cell
# per invocation, 25 MB x 1 run, RWM_PERF_TIMEOUT_S=120 (a DNF is a datum).
# Jitter/control families x5 reps, shal/pol x8 (pre-registered).
# After the bulk loops: the realtime-crown rows (tail_matrix `default` arm
# at c2 clean and at jit15 via RWM_TM_TOPO — the crown-under-jitter row).
#
#   usage: sudo bash adv_battery.sh <seed> [reps]
set -u
cd /home/vibe/raptorpath/raptorpath/tools/l1
source ./lib.sh   # gate forwarding: RWM_FORWARD / rwm_forward_env
set +e            # lib.sh forces `set -euo pipefail`; this driver
                  # runs WITHOUT -e on purpose (per-arm abort tolerance)
SEED_ARG="$1"; REPS="${2:-8}"
JREPS=5   # jitter + control families (pre-registered x5/level)
BIN=/home/vibe/raptorpath/target/release/raptorpath
OUT=/home/vibe/advcells/battery-s${SEED_ARG}.log
DDIR=/home/vibe/advcells/diag
mkdir -p "$DDIR" /home/vibe/advcells
: > "$OUT"
echo "# advcells battery $(date -u +%FT%TZ) seed=$SEED_ARG reps=$REPS jreps=$JREPS" >> "$OUT"
echo "# binary: $(sha256sum $BIN)" >> "$OUT"
echo "# source: $(cat /home/vibe/raptorpath/COMMIT 2>/dev/null || echo unknown)" >> "$OUT"
echo "# kernel: $(uname -r)" >> "$OUT"
lscpu | grep "Model name" >> "$OUT"
(lscpu | grep -E "^Flags" | grep -oE "aes|avx2|pclmulqdq" | sort -u | tr '\n' ' ' >> "$OUT") || true
echo >> "$OUT"

BYTES=25000000
BASEENV="$(rwm_forward_env) RWM_GEN=0 RWM_DIAG=1 RWM_PERF_TIMEOUT_S=120"   # gate forwarding: lib.sh

arm_env() { # arm -> extra env
  case "$1" in
    bbr)     echo "" ;;
    copa)    echo "RWM_QUIC_CC=passthrough" ;;
    compete) echo "RWM_QUIC_CC=passthrough RWM_COPA_COMPETE=1" ;;
  esac
}

run_one() { # cell arm
  local cell="$1" arm="$2" name="$1-$2"
  local envs; envs="$(arm_env "$arm")"
  local t0; t0=$(date +%s)
  echo "=== rep=$REP arm=$name seed=$SEED_ARG env=\"$envs\" cell=$cell bytes=$BYTES $(date -u +%T)" >> "$OUT"
  pkill -x raptorpath 2>/dev/null || true
  # Stale-echo hygiene (the copaclean s7 lesson): remove BOTH endpoint logs
  # BEFORE the run so an aborted invocation can never read the prior arm's.
  rm -f /tmp/adv-c.log /tmp/adv-s.log /tmp/adv-c-clean.log /tmp/adv-q.txt

  # Cell up, seed-7 topo-ping double-abort protocol (discipline 8): retry
  # once, record the aborted invocation loudly, contribute NO datum.
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
      --window-reliable --protocol-hint bulk >/tmp/adv-s.log 2>&1 &
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
      --bytes "$BYTES" --runs 1 >/tmp/adv-c.log 2>&1
  local rc=$?
  [ "$rc" -ne 0 ] && echo "CLIENT-RC $name rep=$REP rc=$rc" >> "$OUT"
  sed 's/\x1b\[[0-9;]*m//g' /tmp/adv-c.log > /tmp/adv-c-clean.log 2>/dev/null || true
  (grep -E '"summary"|"dnf"|"run"' /tmp/adv-c-clean.log | tail -3 >> "$OUT") || true

  # Cell-truth counters BEFORE teardown (bottleneck sent/dropped; the
  # policer's police stats; the pol100 delay stage must show dropped 0).
  bash ./adv_cells.sh counters > /tmp/adv-q.txt 2>/dev/null || true

  echo "RUNTIME $name rep=$REP $(( $(date +%s) - t0 ))s" >> "$OUT"

  # CC liveness (discipline 1): the arm's controller must be the running one.
  local bbrn ptn feedn compt compf
  bbrn=$(grep -c "congestion controller: BBR" /tmp/adv-c-clean.log 2>/dev/null || true)
  ptn=$(grep -c "quinn congestion window is engine-owned" /tmp/adv-c-clean.log 2>/dev/null || true)
  feedn=$(grep -c "Copa delivery feed ACTIVE" /tmp/adv-c-clean.log 2>/dev/null || true)
  compt=$(grep -c "compete=true" /tmp/adv-c-clean.log 2>/dev/null || true)
  compf=$(grep -c "compete=false" /tmp/adv-c-clean.log 2>/dev/null || true)
  echo "LIVENESS arm=$arm bbr=$bbrn pt=$ptn feed=$feedn compete_t=$compt compete_f=$compf" >> "$OUT"
  (grep -oE "copa_wire=[a-z]+|delta=[0-9.]+|cc_pace=[a-z]+|compete=[a-z]+" /tmp/adv-c-clean.log \
    | sort -u | tr '\n' ' ' >> "$OUT") || true
  echo >> "$OUT"
  case "$arm" in
    bbr)
      [ "$bbrn" -eq 0 ] && echo "ARM-LIVENESS-FAIL $name rep=$REP (no BBR echo)" >> "$OUT"
      { [ "$ptn" -gt 0 ] || [ "$feedn" -gt 0 ]; } && echo "ARM-CONTAMINATION $name rep=$REP (copa echo in bbr arm)" >> "$OUT" ;;
    copa)
      { [ "$ptn" -eq 0 ] || [ "$feedn" -eq 0 ]; } && echo "ARM-LIVENESS-FAIL $name rep=$REP (pt=$ptn feed=$feedn)" >> "$OUT"
      [ "$bbrn" -gt 0 ] && echo "ARM-CONTAMINATION $name rep=$REP (bbr echo in copa arm)" >> "$OUT" ;;
    compete)
      { [ "$ptn" -eq 0 ] || [ "$feedn" -eq 0 ] || [ "$compt" -eq 0 ]; } \
        && echo "ARM-LIVENESS-FAIL $name rep=$REP (pt=$ptn feed=$feedn compete_t=$compt)" >> "$OUT"
      [ "$bbrn" -gt 0 ] && echo "ARM-CONTAMINATION $name rep=$REP (bbr echo in compete arm)" >> "$OUT" ;;
  esac

  # Per-run map row: summary + queue/loss/compete profile + cell-truth drops.
  python3 - "$cell" "$arm" "$SEED_ARG" "$REP" <<'EOF' >> "$OUT" 2>&1 || echo "ADVRESULT-PARSE-FAIL $name rep=$REP" >> "$OUT"
import json, re, sys
cell, arm, seed, rep = sys.argv[1:5]

mbps = secs = None; dnf = False
try:
    for line in open("/tmp/adv-c-clean.log", errors="replace"):
        line = line.strip()
        if line.startswith("{"):
            try: j = json.loads(line)
            except Exception: continue
            if j.get("run") == 1:
                if j.get("dnf"): dnf = True
                else: mbps, secs = j.get("mbps"), j.get("seconds")
except FileNotFoundError:
    dnf = True

# DIAG: wireQ/appQ (wrtt/rtt minus rtp), engine loss estimate pl, retx,
# compete counters. Steady state = per-path DIAG lines 4+ (pooling rule).
wq, aq, pls = [], [], []
retx = 0
cmp_n = cmp_c = cmp_sw = 0
pat = re.compile(r"rtt=(\d+)/wrtt=(\d+)/rtp(\d+)ms")
ppat = re.compile(r"pl=([0-9.]+)")
rpat = re.compile(r"retx=(\d+)")
cpat = re.compile(r"cmp=([CD])(\d+)/([0-9.]+)")
nline = 0
try:
    for line in open("/tmp/adv-c-clean.log", errors="replace"):
        m = pat.search(line)
        if m:
            nline += 1
            if nline >= 4:
                rtt, wrtt, rtp = map(int, m.groups())
                if wrtt > 0 and rtp > 0:
                    wq.append(max(wrtt - rtp, 0)); aq.append(max(rtt - rtp, 0))
                p = ppat.search(line)
                if p: pls.append(float(p.group(1)))
        r = rpat.search(line)
        if r: retx = max(retx, int(r.group(1)))
        c = cpat.search(line)
        if c:
            cmp_n += 1
            cmp_sw = max(cmp_sw, int(c.group(2)))
            if c.group(1) == "C": cmp_c += 1
except FileNotFoundError:
    pass

# Cell-truth: tc -s counters (== CLI0 / == SRV0 / == SRV0-INGRESS sections).
# Bottleneck per cell: c2ctl/jit* = CLI0 netem; shal8 = CLI0 child netem
# (tbf root recorded separately); pol100 = the ingress police stats, with
# the CLI0 delay-stage netem recorded (must be 0 drops).
sec = None
stats = {}   # (section, kind) -> (sent_pkt, dropped)
kind = None
# NOTE: qdisc stats print "pkt", the police action stats print "pkts"
# (validated live on the VM, 2026-08-06) — accept both.
sent_re = re.compile(r"Sent (\d+) bytes (\d+) pkts? \(dropped (\d+)")
try:
    for line in open("/tmp/adv-q.txt", errors="replace"):
        if line.startswith("== "):
            sec = line[3:].split()[0]; kind = None; continue
        mq = re.match(r"\s*qdisc (\w+)", line)
        if mq: kind = mq.group(1)
        if re.match(r"\s*police ", line): kind = "police"
        ms = sent_re.search(line)
        if ms and sec and kind:
            k = (sec, kind)
            s0, d0 = stats.get(k, (0, 0))
            stats[k] = (s0 + int(ms.group(2)), d0 + int(ms.group(3)))
except FileNotFoundError:
    pass

if cell == "pol100":
    bn = stats.get(("SRV0-INGRESS", "police")); bnk = "police"
elif cell == "shal8":
    bn = stats.get(("CLI0", "netem")); bnk = "netem-child"
else:
    bn = stats.get(("CLI0", "netem")); bnk = "netem"
bn_sent, bn_drop = (bn or (None, None))
frac = None
if bn_sent is not None and (bn_sent + bn_drop) > 0:
    frac = bn_drop / (bn_sent + bn_drop)
delay_drop = stats.get(("CLI0", "netem"), (None, None))[1] if cell == "pol100" else None
tbf_drop = stats.get(("CLI0", "tbf"), (None, None))[1] if cell == "shal8" else None

def q(v, p):
    if not v: return None
    v = sorted(v); return v[min(len(v) - 1, int(round(p * (len(v) - 1))))]

print("ADVRESULT " + json.dumps({
    "cell": cell, "arm": arm, "seed": int(seed), "rep": int(rep),
    "dnf": dnf, "mbps": mbps, "seconds": secs,
    "wq_p50": q(wq, 0.5), "wq_p90": q(wq, 0.9),
    "aq_p50": q(aq, 0.5), "aq_p90": q(aq, 0.9),
    "pl_max": max(pls) if pls else None, "retx": retx,
    "cmp_lines": cmp_n, "cmp_switches": cmp_sw,
    "cmp_c_frac": round(cmp_c / cmp_n, 3) if cmp_n else None,
    "bn_kind": bnk, "bn_sent_pkt": bn_sent, "bn_drop": bn_drop,
    "bn_drop_frac": round(frac, 4) if frac is not None else None,
    "delay_stage_drop": delay_drop, "tbf_drop": tbf_drop,
}))
EOF

  cp /tmp/adv-c.log "$DDIR/${name}-s${SEED_ARG}-r${REP}-c.log" 2>/dev/null || true
  cp /tmp/adv-s.log "$DDIR/${name}-s${SEED_ARG}-r${REP}-s.log" 2>/dev/null || true
  cp /tmp/adv-q.txt "$DDIR/${name}-s${SEED_ARG}-r${REP}-q.txt" 2>/dev/null || true
  pkill -x raptorpath 2>/dev/null || true
  bash ./adv_cells.sh down >/dev/null 2>&1 || true
}

if pgrep -x raptorpath >/dev/null 2>&1; then
  echo "BUSY: raptorpath already running -- aborting" | tee -a "$OUT" >&2
  exit 3
fi

for REP in $(seq 1 "$REPS"); do
  if [ "$REP" -le "$JREPS" ]; then
    run_one c2ctl bbr;  run_one c2ctl copa
    run_one jit0  bbr;  run_one jit0  copa
    run_one jit5  bbr;  run_one jit5  copa
    run_one jit15 bbr;  run_one jit15 copa
    run_one jit25 bbr;  run_one jit25 copa
  fi
  run_one shal8  bbr; run_one shal8  copa; run_one shal8  compete
  run_one pol100 bbr; run_one pol100 copa; run_one pol100 compete
done

# Realtime-crown rows (shipped default machine, tail_matrix `default` arm):
# clean c2 control + the jit15 adversarial cell (RWM_TM_TOPO=adv_cells.sh).
echo "=== TAILROW c2-clean default $(date -u +%T)" >> "$OUT"
SEED=$SEED_ARG RWM_TM_ARMS='default' bash ./tail_matrix.sh c2 8 >> "$OUT" 2>&1 || echo "TAILROW-FAIL c2" >> "$OUT"
echo "=== TAILROW jit15 default $(date -u +%T)" >> "$OUT"
SEED=$SEED_ARG RWM_TM_ARMS='default' RWM_TM_TOPO=./adv_cells.sh bash ./tail_matrix.sh jit15 8 >> "$OUT" 2>&1 || echo "TAILROW-FAIL jit15" >> "$OUT"

# Arm-liveness (discipline 7): an arm with zero captured rows fails LOUDLY.
echo "--- ARMCOUNTS" >> "$OUT"
for a in c2ctl-bbr c2ctl-copa jit0-bbr jit0-copa jit5-bbr jit5-copa \
         jit15-bbr jit15-copa jit25-bbr jit25-copa \
         shal8-bbr shal8-copa shal8-compete pol100-bbr pol100-copa pol100-compete; do
  hdr=$(grep -c "arm=$a " "$OUT" || true)
  res=$(grep -c "\"cell\": \"${a%-*}\", \"arm\": \"${a##*-}\"" "$OUT" || true)
  echo "ARMCOUNT $a headers=$hdr results=$res" >> "$OUT"
  [ "$res" -eq 0 ] && echo "ARM-VANISHED $a" >> "$OUT"
done
echo "BATTERY-DONE seed=$SEED_ARG $(date -u +%FT%TZ)" >> "$OUT"
echo BATTERY-DONE-$SEED_ARG
