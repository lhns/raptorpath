#!/bin/bash
# knobs: GEN GR PIPE STORE INFLIGHT WAIT OOO OOORET REACT
# usage: bash pmeas2.sh <tag> <scen> <reps> <bytes> <arq|fec|fecpacer>
cd ~/l1
tag="$1"; scen="$2"; reps="$3"; bytes="$4"; arm="$5"
base="RWM_OOO=${OOO:-0} RWM_GEN=${GEN:-384} RWM_GEN_R=${GR:-0.15} RWM_STORE=${STORE:-2048} RWM_PIPELINE=${PIPE:-2} RWM_GEN_INFLIGHT=${INFLIGHT:-1024} RWM_PFRAC=1 RWM_FDIAG=1 RWM_CC_PACE=1 RWM_CC_PACE_HR=1.1 RWM_REACT_CAP=${REACT:-1.0} RWM_REPAIR_WAIT=${WAIT:-40} RWM_EXTRA=--window-systematic-repair"
[[ -n "${OOORET:-}" ]] && base="$base RWM_OOO_RETAIN=$OOORET"
case "$arm" in
  arq) envs="";;
  fec) envs="$base";;
  fecpacer) envs="$base RWM_PROACTIVE_PACER=1";;
esac
while pgrep -x raptorpath >/dev/null; do sleep 2; done
line=$(env $envs timeout 700 sudo -E bash perf_rwm_c.sh "$scen" "$scen" bulk "$bytes" "$reps" single 2>&1 | grep -E "\"summary\"")
mbps=$(echo "$line" | grep -oE "\"mean_mbps\":[0-9.]+" | cut -d: -f2)
dnf=$(echo "$line" | grep -oE "\"dnf\":[0-9]+" | cut -d: -f2)
# Robust: final cumulative PFRAC = line with MAX total_coded across both logs.
pf=$(cat /tmp/rwm-s.log /tmp/rwm-c.log 2>/dev/null | grep -E "PFRAC" | sort -t= -k4 -n | tail -1)
pcod=$(echo "$pf" | grep -oE "proactive_coded=[0-9]+" | cut -d= -f2)
rcod=$(echo "$pf" | grep -oE "recovery_coded=[0-9]+" | cut -d= -f2)
pfrac=$(echo "$pf" | grep -oE "proactive_fraction=[0-9.]+" | cut -d= -f2)
# Robust: final FDIAG = line with MAX frontier across both logs.
fd=$(cat /tmp/rwm-s.log /tmp/rwm-c.log 2>/dev/null | grep -E "\[FDIAG\]" | sed -E "s/.*frontier=([0-9]+).*/\1 &/" | sort -n | tail -1 | cut -d" " -f2-)
pas=$(echo "$fd" | grep -oE "present_at_stall=[0-9]+" | cut -d= -f2)
decn=$(echo "$fd" | grep -oE "DECODE n=[0-9]+" | grep -oE "[0-9]+")
echo "PMEAS tag=$tag scen=$scen arm=$arm mbps=${mbps:-NA} dnf=${dnf:-NA} pfrac=${pfrac:-NA} pcod=${pcod:-NA} rcod=${rcod:-NA} present=${pas:-NA}/${decn:-NA}"
echo "  FD: ${fd:-none}"
