#!/bin/bash
# usage: simdet_child.sh BIN OUTDIR NAME SCEN PORT
# SCEN: shed6 (current sim_shed_reach scenario) | nat30 (old 5% 30MB) | basic (sim_determinism basic)
BIN=$1; OUT=$2; NAME=$3; SCEN=$4; PORT=$5
mkdir -p $OUT
export SIM_CHILD=1 SIM_OUT=$OUT/$NAME.trace SIM_PORT=$PORT SIM_PATHS=1 SIM_JITTER_MS=4 RWM_DIAG=1
case $SCEN in
 shed6) export SIM_SEED=42 SIM_BYTES=6000000 SIM_HINT=realtime SIM_RELIABLE=false SIM_LOSS=0.025 SIM_LIMIT_S=40 \
   RWM_TEST_TS_SEQ=12087 RWM_TEST_TS_DROP=dead RWM_TEST_TS_DEAD_MS=1500 RWM_TEST_TS_SHED_ALL=1 RWM_TEST_TS_BUDGET_FRAC=0.05;;
 nat30) export SIM_SEED=42 SIM_BYTES=30000000 SIM_HINT=realtime SIM_RELIABLE=false SIM_LOSS=0.05 SIM_LIMIT_S=40 \
   RWM_TEST_TS_SEQ=999999999 RWM_TEST_TS_SHED_ALL=1 RWM_TEST_TS_BUDGET_FRAC=0.05;;
 basic) export SIM_SEED=5 SIM_BYTES=2000000 SIM_HINT=auto SIM_RELIABLE=- SIM_LOSS=0.01 SIM_LIMIT_S=30;;
esac
"$BIN" child --exact --nocapture --test-threads=1 > $OUT/$NAME.log 2>&1
echo "$NAME rc=$? $(grep '^\[SIM\]' $OUT/$NAME.log) shed=$(grep -c '^\[TSEAM\] ev=shed ' $OUT/$NAME.log)"
