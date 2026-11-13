#!/bin/bash
# post-hoc checks for status 52 (Fable): holeclass loop on both trees, alloc_per_symbol --ignored on both.
set -u
. ~/s52/src/raptorpath/tools/l1/lib_battery.sh
install_lock_traps && take_lock /tmp/rwm-vm.lock && take_lock ~/rp.lock || { echo "LOCK-FAIL"; exit 1; }
O=~/post52; mkdir -p $O; C=~/.cargo/bin/cargo
echo "START $(date -u +%FT%TZ)"
HB=~/post52/tgt-base/debug/deps/holeclass_reachability-b746e82c55e6af0d; [ -x "$HB" ] || { echo NO-HB; exit 1; }
AB=~/post52/tgt-base/release/deps/alloc_per_symbol-b70b349fd673c735; [ -x "$AB" ] || { echo NO-AB; exit 1; }
HC=~/s52/target-tests/debug/deps/holeclass_reachability-b746e82c55e6af0d
AC=~/s52/target-tests/release/deps/alloc_per_symbol-b70b349fd673c735
for t in base cand; do
  if [ $t = base ]; then A=$AB; D=~/s52/base/src/raptorpath; else A=$AC; D=~/s52/src/raptorpath; fi
  (cd $D && timeout 900 $A --ignored --nocapture > $O/alloc-$t.log 2>&1); echo "ALLOC $t rc=$?"
done
for i in $(seq 1 10); do
  for t in base cand; do
    if [ $t = base ]; then H=$HB; D=~/s52/base/src/raptorpath; else H=$HC; D=~/s52/src/raptorpath; fi
    (cd $D && timeout 600 $H --test-threads=2 > $O/hc-$t-$i.log 2>&1); rc=$?
    echo "HC $t iter=$i rc=$rc $(grep -oE 'fed=[0-9]+ .*age_n=[0-9]+' $O/hc-$t-$i.log | head -0)$(grep -c 'cannot precede' $O/hc-$t-$i.log)"
  done
done
echo "POST52-DONE $(date -u +%FT%TZ)"
