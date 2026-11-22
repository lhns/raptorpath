#!/bin/bash
# Plan 67 (status 67, VM batch 1: docs/derivations/vm-batch-1-registration.md; plan67.py, s67_score.py, tun67_battery.sh): the SESSION ENVELOPE of LD_PLAN = 67pre | 67 | 67g | 67x.
# SOURCED by loopdiag_run_all.sh (P67=1) after the locks, the launch precondition, the budget guard, the tools gates and the definitions of run_suites / reuse_prior_tools / tools_manifest; it never returns (every path exits).
# It is a separate file because plan 67 is the first plan with SEVEN binaries: the two-seat BIN / BIN_BASE machinery of run_all is not stretched, the per-seat variables LD_BIN_<SEAT> / LD_SHA_<SEAT> / LD_TOOLS_<SEAT> are.
#
#   LD_PLAN=67pre               session 0, the pre-flight (registration 2.2, 6.0; no battery, no build): after the seat pins below it branches into loopdiag_p67pre.sh, which never returns. Seats B0, S1, S2 (LD_DROP_S2=1: no S2 step).
#                               It refuses LD_REHEARSE=1 (run_all: not in its REHEARSE list). `67p` is plan 67's R2 battery plan (plan67.STAGE_PLAN), so session 0 has its own token, as status 60's 60-pre.
#   LD_PLAN=67  LD_REHEARSE=1   session 1, the rehearsal: build the seats (shas RECORDED, or checked when LD_BIN_SHA_<SEAT> is given), the seat identity checks, the tools tests, the suite-reuse HIT check against session 0, and one smoke per stage;
#                               no scored row, no battery. Ends LD-ALL-REHEARSAL-DONE with $RUN/AMENDMENT2.txt (the shas, the twin decision, the dump-twin need) for the second amendment.
#   LD_PLAN=67g                 session 2: G1 (S1 against B0) then G2 (S2 against S1), plan 41d twice (LD_WITNESS67 = g1 | g2).
#   LD_PLAN=67x                 session 3: A (with A1), R1 (with A1-R), T, R2 (with the test-seams twins), the dump twins (LD_DUMP67=1 only), the pool block.
# Seats (plan67.SEATS): B0 = vmb-base, S1 = vmb-s1, S2 = vmb-s2, P1 / P2 = S1 / S2 + the pacer (their own tools tree carries the lib.sh forward line), P1T / P2T = P1 / P2 built --release --features test-seams.
# Source trees: $LD_ROOT/seats/<B0|S1|S2|P1|P2>/src (a git archive of the repo root; COMMIT beside it, $LD_ROOT/seats/<tree>/COMMIT). The commit pin of a seat is LD_PIN_<SEAT>: REQUIRED for every seat, no default (ABORT-SEAT when unset).
# Test seams (LD_HARD_MIN_S, R67_T1_S, LD_ROW_BYTES, LD_BLOCK_ROW_S and the fast-mode constants of plan67.py / G_ROWS67): loopdiag_run_all.sh aborts ABORT-SEAM before the locks unless LD_TEST_SEAMS=1 (the stub marker).
# Binaries: a launch (67g / 67x) NEVER builds: LD_BIN_PREBUILT_<SEAT> (the rehearsal's $RUN/bins67/<seat>-raptorpath) and LD_BIN_SHA_<SEAT> (the second amendment's pin) are required for every seat the session uses (ABORT-BUILD).
# Abort classes (registration 5.4): ABORT-SEAT (a commit pin or an identity string / tools-diff witness fails), ABORT-BUILD (a binary sha against its pin, a missing prebuilt), ABORT-SUITES (the suite-reuse manifest MISSes), ABORT-TESTS (the tools
# tests), ABORT-SMOKE, ABORT-BUDGET. A seat of the S2 family (S2, P2, P2T) failing aborts the session with the line "relaunch with LD_DROP_S2=1": guard 1 then runs alone (registration 5.2).
# Coordinator switches: LD_DROP_S2=1 (S2, P2, P2T, G2 and every S2 arm gone), LD_G2_FAIL67=1 (session 3 after a G2 FAIL or an S06-FAIL in G2: the S2 arms, R2, the pool block and the dump twins dropped, A1 / A1-R / T kept),
# LD_DUMP67=1 (the dump twins run: the rehearsal found the [SUCC] line without the close times), LD_CUT67=dump (the only registered cap cut; the pool block and R2 are never cut).

P67ALL=(B0 S1 S2 P1 P2 P1T P2T)
G_ROWS67=96     # stage G's registered design (test_stubrun's fast mode shrinks it)
[ "$PLAN" != "67" ] || [ "$REHEARSE" = 1 ] || fail_all "ABORT-REHEARSE plan 67 is the rehearsal: it needs LD_REHEARSE=1; NOTHING WAS RUN"
export LD_G2_FAIL67="${LD_G2_FAIL67:-0}" LD_DUMP67="${LD_DUMP67:-0}" LD_CUT67="${LD_CUT67:-}"
[ "$LD_G2_FAIL67" != 1 ] || LD_DROP_S2=1
export LD_DROP_S2="${LD_DROP_S2:-0}"
case ",$LD_CUT67," in ,,|,dump,) ;; *) fail_all "ABORT-CUT plan 67: LD_CUT67=$LD_CUT67 (the registered cap cut is the dump twins only: R2, its twins and the pool block are never cut); NOTHING WAS RUN" ;; esac

# ── the seats this session uses ──
case "$PLAN" in 67pre) P67NEED=(B0 S1 S2) ;; 67) P67NEED=(B0 S1 S2 P1 P2 P1T P2T) ;; 67g) P67NEED=(B0 S1 S2) ;; 67x) P67NEED=(B0 S1 S2 P1 P2 P1T P2T) ;; esac
[ "$LD_DROP_S2" != 1 ] || { P67TMP=(); for _s in "${P67NEED[@]}"; do case "$_s" in S2|P2|P2T) ;; *) P67TMP+=("$_s") ;; esac; done; P67NEED=("${P67TMP[@]}"); }
[ "$PLAN" != "67x" ] || [ "$LD_G2_FAIL67" != 1 ] || { P67TMP=(); for _s in "${P67NEED[@]}"; do case "$_s" in P1|P2|P1T|P2T) ;; *) P67TMP+=("$_s") ;; esac; done; P67NEED=("${P67TMP[@]}"); }
s2fam() { case "$1" in S2|P2|P2T) return 0 ;; *) return 1 ;; esac; }
P67END="; NOTHING WAS RUN"      # loopdiag_p67pre.sh clears it: by the time a session-0 step fails, earlier steps have run
seat_fail() { # seat message...   (the S2 family: guard 2 stops, guard 1 can still run alone)
  local s="$1"; shift
  if s2fam "$s"; then fail_all "$* (seat $s is in the S2 family: guard 2 stops; relaunch with LD_DROP_S2=1 to run guard 1 alone)$P67END"; fi
  fail_all "$*$P67END"
}
era "LD-ALL plan $PLAN seats: ${P67NEED[*]} (LD_DROP_S2=$LD_DROP_S2 LD_G2_FAIL67=$LD_G2_FAIL67 LD_DUMP67=$LD_DUMP67 LD_CUT67=${LD_CUT67:-none})"
P67STAGES=($(python3 ./plan67.py stages "$PLAN"))
era "LD-ALL plan $PLAN stages: ${P67STAGES[*]}"

# src_manifest67: the reference manifest of a seat tree, one definition for session 0 (loopdiag_p67pre.sh writes it) and for section 5 below (compares against it). It covers tools/ci/flake_rule.py (ADR-0080's rule: run_all and suites_run.sh
#       read it), through THIS function only, so no other plan's committed reference manifest changes.
src_manifest67() { # tree root
  ( cd "$1" || exit 1
    { ls Cargo.toml Cargo.lock ./*/Cargo.toml 2>/dev/null | sed 's#^\./##'
      find raptorpath/src raptorpath/tests raptorpath-math/src raptorpath-core/src raptorpath-wasm/src gf256/src vendor tools/ci/flake_rule.py -type f 2>/dev/null; } | LC_ALL=C sort -u | xargs -r sha256sum | sed -E 's/^([0-9a-f]{64}) [ *]/\1  /' )
}
# ── 1. the commit pins and the source trees (ABORT-SEAT) ──
declare -A P67TREE P67PIN
for _s in "${P67NEED[@]}"; do
  P67TREE[$_s]="$(python3 -c "import plan67; print(plan67.SEATS['$_s']['tree'])")"
  read -r _penv _ <<< "$(python3 ./plan67.py pin "$_s")"
  _pv="${!_penv:-}"
  [ -n "$_pv" ] || seat_fail "$_s" "ABORT-SEAT plan 67 seat $_s: $_penv is unset (every seat's commit pin is required, none has a default: the pin amendment names it)"
  P67PIN[$_s]="$_pv"
  _cf="$ROOT/seats/${P67TREE[$_s]}/COMMIT"
  case "$(cat "$_cf" 2>/dev/null)" in "$_pv"*) ;; *) seat_fail "$_s" "ABORT-SEAT plan 67 seat $_s: $_cf is '$(cat "$_cf" 2>/dev/null)', not the pin $_pv ($_penv)" ;; esac
done
era "LD-ALL seat commits: $(for _s in "${P67NEED[@]}"; do echo -n "$_s=${P67PIN[$_s]} "; done)"
[ "$PLAN" != "67pre" ] || source "$HERE/loopdiag_p67pre.sh"      # session 0: every path exits

# ── 2. the binaries: built (the rehearsal) or prebuilt (a launch), installed onto the root-owned tmpfs tree, sha-checked against the pins ──
sudo -n mkdir -p "$SHM" "$RUN" && sudo -n chmod 1777 "$SHM" "$RUN" || fail_all "ABORT-SHM cannot create the root-owned tmpfs tree"
declare -A P67BIN P67SHA P67TOOLS
mkdir -p "$RUN/bins67"
T0=$(date +%s)
: > "$RUN/BINSHA.txt"
for _s in "${P67NEED[@]}"; do
  _tree="${P67TREE[$_s]}"
  _feat="$(python3 -c "import plan67; print(plan67.SEATS['$_s']['features'])")"
  _pre="LD_BIN_PREBUILT_$_s"; _pin="LD_BIN_SHA_$_s"
  _out="$SHM/bin-$_s/raptorpath"
  sudo -n mkdir -p "$SHM/bin-$_s" && sudo -n chmod 1777 "$SHM/bin-$_s" || fail_all "ABORT-SHM cannot create $SHM/bin-$_s"
  if [ -n "${!_pre:-}" ]; then
    [ -n "${!_pin:-}" ] || fail_all "ABORT-BUILD plan 67: $_pre needs $_pin (the second amendment's pin); NOTHING WAS RUN"
    [ -f "${!_pre}" ] && [ -x "${!_pre}" ] || fail_all "ABORT-BUILD plan 67: $_pre=${!_pre} is not an executable file; NOTHING WAS RUN"
    _src="${!_pre}"; era "LD-ALL seat $_s: binary prebuilt from $_src (no build)"
  elif [ "$REHEARSE" = 1 ]; then
    _sdir="$ROOT/seats/$_tree/src/raptorpath"; _tgt="$ROOT/target-$_s"
    [ -d "$_sdir" ] || seat_fail "$_s" "ABORT-BUILD plan 67 seat $_s: no source tree $_sdir"
    [ ! -e "$_sdir/target" ] && [ ! -e "$_tgt" ] || fail_all "ABORT-BUILD plan 67 seat $_s: $_sdir/target or $_tgt is not fresh"
    ( cd "$_sdir" && CARGO_TARGET_DIR="$_tgt" CARGO_BUILD_JOBS=4 "$CARGO" build --release --bin raptorpath ${_feat:+--features $_feat} ) > "$ROOT/build-$_s.log" 2>&1 \
      || seat_fail "$_s" "ABORT-BUILD plan 67 seat $_s ($ROOT/build-$_s.log)"
    _src="$_tgt/release/raptorpath"
  else
    fail_all "ABORT-BUILD plan 67 launches never build: $_pre (the rehearsal's $RUN/bins67/$_s-raptorpath) and $_pin are required for seat $_s; NOTHING WAS RUN"
  fi
  sudo -n install -m 0755 -o root -g root "$_src" "$_out" || fail_all "ABORT-BUILD plan 67 seat $_s: copy"
  P67BIN[$_s]="$_out"; P67SHA[$_s]="$(sha256sum "$_out" | cut -d' ' -f1)"
  echo "${P67SHA[$_s]}  $_out  seat=$_s commit=${P67PIN[$_s]} features=${_feat:-none}" >> "$RUN/BINSHA.txt"
  cp "$_src" "$RUN/bins67/$_s-raptorpath" || fail_all "ABORT-BUILD plan 67: cannot keep $_s's binary in $RUN/bins67"
  if [ -n "${!_pin:-}" ]; then
    [ "${P67SHA[$_s]}" = "${!_pin}" ] || seat_fail "$_s" "ABORT-BUILD plan 67 seat $_s: built / installed ${P67SHA[$_s]} != pinned $_pin ${!_pin}"
    era "LD-ALL seat $_s binary ${P67SHA[$_s]} equals its pin"
  else
    era "LD-ALL seat $_s binary ${P67SHA[$_s]} RECORDED (no $_pin: the rehearsal records it for the second amendment)"
  fi
  case "$_s" in P1|P2|P1T|P2T) P67TOOLS[$_s]="$ROOT/seats/$_tree/src/tools/l1" ;; esac
done
era "LD-ALL seats installed wall=$(( $(date +%s) - T0 ))s"
for _a in "${P67NEED[@]}"; do for _b in "${P67NEED[@]}"; do
  [[ "$_a" < "$_b" ]] && [ "${P67SHA[$_a]}" = "${P67SHA[$_b]}" ] && seat_fail "$_a" "ABORT-BUILD plan 67: seats $_a and $_b are the same binary (${P67SHA[$_a]})"
done; done

# ── 3. the seat identity: the strings each binary must and must not hold (ABORT-SEAT, before any row, a rehearsal included; S1 and S2 both must carry `pfec=`) ──
for _s in "${P67NEED[@]}"; do
  while read -r _kind _str; do
    case "$_kind" in
      need) grep -aqF -- "$_str" "${P67BIN[$_s]}" || seat_fail "$_s" "ABORT-SEAT plan 67 seat $_s: the binary has no '$_str' string: it is not the registered build" ;;
      forbid) ! grep -aqF -- "$_str" "${P67BIN[$_s]}" || seat_fail "$_s" "ABORT-SEAT plan 67 seat $_s: the binary carries '$_str': it is not the registered build" ;;
    esac
  done < <(python3 ./plan67.py strings "$_s")
done
era "LD-ALL seat identity strings hold for ${P67NEED[*]}"

# ── 4. the tools-diff witness: every seat runs one harness; the pacer seats' own tools tree differs by the one lib.sh forward line (registration 1.2, 1.3) ──
tools_files67() { ( cd "$1" && find . -type f ! -name '*.pyc' ! -path '*/__pycache__/*' | LC_ALL=C sort | xargs -r sha256sum | sed -E 's/^([0-9a-f]{64}) [ *]/\1  /' ); }
P67HT="$(tools_files67 "$HERE")"
declare -A P67DONETREE
for _s in "${P67NEED[@]}"; do
  _tree="${P67TREE[$_s]}"; [ -z "${P67DONETREE[$_tree]:-}" ] || continue; P67DONETREE[$_tree]=1
  _td="$ROOT/seats/$_tree/src/tools/l1"
  [ -d "$_td" ] || seat_fail "$_s" "ABORT-SEAT plan 67 seat $_s: no tools tree $_td"
  _tt="$(tools_files67 "$_td")"
  case "$_tree" in
    P1|P2)
      _dl="$(diff <(echo "$P67HT") <(echo "$_tt") | grep -E '^[<>]' | awk '{print $3}' | sort -u | tr '\n' ' ')"
      [ "$_dl" = "./lib.sh " ] || seat_fail "$_s" "ABORT-SEAT plan 67 tools-diff witness: $_tree's tools/l1 differs from the harness's in '$_dl', not in lib.sh alone"
      sed 's/ RWM_PERF_RATE_MBIT//' "$_td/lib.sh" | cmp -s - "$HERE/lib.sh" || seat_fail "$_s" "ABORT-SEAT plan 67 tools-diff witness: $_tree's lib.sh differs from the harness's by more than the RWM_PERF_RATE_MBIT forward line"
      grep -aq 'RWM_PERF_RATE_MBIT' "$_td/lib.sh" || seat_fail "$_s" "ABORT-SEAT plan 67 seat $_s: $_td/lib.sh does not forward RWM_PERF_RATE_MBIT" ;;
    *) [ "$_tt" = "$P67HT" ] || seat_fail "$_s" "ABORT-SEAT plan 67 tools-diff witness: $_tree's tools/l1 is not byte-identical to the harness's ($(diff <(echo "$P67HT") <(echo "$_tt") | grep -c '^[<>]') manifest lines)" ;;
  esac
done
era "LD-ALL tools-diff witness: the seats' tools trees equal the harness's (the pacer trees by the one lib.sh line)"

# ── 5. suite reuse (the function src_manifest67 is defined above section 1: session 0 writes the reference manifests with it) ──
SD="$RUN/suites"; mkdir -p "$SD"
for _s in S1 S2; do
  [[ " ${P67NEED[*]} " == *" $_s "* ]] || continue
  _rf="LD_REF_MANIFEST_$_s"
  [ -n "${!_rf:-}" ] && [ -s "${!_rf}" ] || seat_fail "$_s" "ABORT-SUITES plan 67: no reference manifest for seat $_s ($_rf: session 0's record of the suites run on that tree)"
  src_manifest67 "$ROOT/seats/${P67TREE[$_s]}/src" > "$SD/SRC-MANIFEST-$_s.now"
  if cmp -s "${!_rf}" "$SD/SRC-MANIFEST-$_s.now"; then
    era "LD-ALL suite-reuse HIT seat $_s: src_manifest67 (the engine crates, vendor/, the Cargo files and tools/ci/flake_rule.py) byte-identical to ${!_rf} ($(wc -l < "$SD/SRC-MANIFEST-$_s.now") files)"
  else
    era "LD-ALL suite-reuse MISS seat $_s: $(diff "${!_rf}" "$SD/SRC-MANIFEST-$_s.now" | grep -c '^[<>]') manifest lines differ"
    seat_fail "$_s" "ABORT-SUITES plan 67 suite-reuse MISS: seat $_s's tree is not the tree whose suites ran in session 0 (reference manifest ${!_rf})"
  fi
done

# ── 6. the tools tests (python and bash): a rehearsal always runs them and writes TOOLS-TESTS-OK + TOOLS-MANIFEST.sha256; a launch reuses them (LD_PRIOR_TOOLS) on a byte-identical tools/l1, else runs them ──
ENGINE_SUITES=0; ENGINE_WHY="(plan 67: the engine suites ran in session 0 on S1 and on S2; the reference-manifest HIT above stands for them)"
if [ "$REHEARSE" != 1 ] && [ -n "${LD_PRIOR_TOOLS:-}" ] && reuse_prior_tools; then
  echo "ENGINE-SUITES-REUSED $ENGINE_WHY" > "$SD/TESTS.txt"; echo "TOOLS-TESTS-REUSED $PRIOR_TOOLS_DIR" >> "$SD/TESTS.txt"
else
  run_suites || fail_all "ABORT-TESTS ($SD/TESTS.txt)"
fi
era "LD-ALL tools tests ok"

# ── 7. one stage ──
# returns 0 = the stage ran to the end (its verdicts are in score.txt), 1 = it did not complete, 2 = the rehearsal's smoke passed, 6 = the block-1 rule stopped it (FAILED-<stage>; the next stage still runs),
# 7 = cut at LD_HARD (TRUNCATED: the battery or its replacement run was killed by the timeout; scored, every PASS reads TRUNCATED), 8 = not started: too little time left before LD_HARD (ABORT-BUDGET).
# 7 and 8 are not failures (measurement-discipline: the truncation sentinel is not a failure): the session ends TRUNCATED-ALL unless another stage failed.
stage67() { # name
  local name="$1" bplan dir smk scr chk bto brc ok=0 cand="" base="" w="" seats="" s v sbrc smrc nrows T1
  bplan="$(python3 -c "import plan67; print(plan67.STAGE_PLAN.get('$name', ''))")"
  dir="$RUN/$(python3 -c "import plan67; print(plan67.STAGE_DIR['$name'])")"
  mkdir -p "$dir"
  case "$name" in
    g1) cand=S1; base=B0; w=g1 ;;
    g2) cand=S2; base=S1; w=g2 ;;
    t) seats="B0 S1" ;;
    a|r1) seats="B0 S1 S2" ;;
    r2) seats="P1 P2 P1T P2T" ;;
    pool|dump) seats="S1 S2" ;;
  esac
  [ "$LD_DROP_S2" != 1 ] || seats="${seats//S2/}"
  [ "$LD_DROP_S2" != 1 ] || seats="${seats//P2T/}"; [ "$LD_DROP_S2" != 1 ] || seats="${seats//P2/}"
  local SENV=(LD_OUTDIR="$dir" LD_HARD="${LD_HARD:-}" LD_REHEARSE="$REHEARSE" LD_DROP_S2="$LD_DROP_S2" LD_DUMP67="$LD_DUMP67" LD_WITNESS54=0 LD_WITNESS64=0)
  [ "${LD_TEST_SEAMS:-}" != 1 ] || SENV+=(LD_BLOCK_ROW_S="${LD_BLOCK_ROW_S:-}" LD_ROW_BYTES="${LD_ROW_BYTES:-}")      # the test seams reach a battery only under the explicit stub marker
  if [ -n "${w:-}" ]; then
    SENV+=(LD_BIN="${P67BIN[$cand]}" LD_SHA="${P67SHA[$cand]}" LD_BIN_BASE="${P67BIN[$base]}" LD_SHA_BASE="${P67SHA[$base]}" LD_BASE_SHA="${P67PIN[$base]}" LD_PLAN=41d LD_WITNESS67="$w")
    SENV+=(LD_SHA_G_CAND="${P67SHA[$cand]}" LD_SHA_G_BASE="${P67SHA[$base]}")
  else
    SENV+=(LD_BIN="${P67BIN[S1]}" LD_SHA="${P67SHA[S1]}" LD_PLAN="$bplan" LD_SEATS67="$seats")
    for s in $seats; do SENV+=("LD_BIN_$s=${P67BIN[$s]}" "LD_SHA_$s=${P67SHA[$s]}"); [ -z "${P67TOOLS[$s]:-}" ] || SENV+=("LD_TOOLS_$s=${P67TOOLS[$s]}"); done
  fi
  # which parsers / scorers read this stage
  case "$name" in
    g1|g2) smk=(smokeg "$w"); scr=(scoreg "$w") ;;
    t) smk=(smokeT); scr=(tun) ;;
    *) smk=(smoke "$bplan"); scr=(score "$bplan") ;;
  esac
  local runner=loopdiag_battery.sh
  [ "$name" != "t" ] || { runner=tun67_battery.sh; SENV=(LD_OUTDIR="$dir" LD_BIN_B0="${P67BIN[B0]}" LD_SHA_B0="${P67SHA[B0]}" LD_BIN_S1="${P67BIN[S1]}" LD_SHA_S1="${P67SHA[S1]}" LD_HARD="${LD_HARD:-}" LD_DROP_S2="$LD_DROP_S2"); }
  if [ "$REHEARSE" = 1 ]; then
    era "LD-ALL stage $name smoke; $(prec_samples | cut -d" " -f2-) io=$(grep '^full' "$PROCFS"/pressure/io)"
    # the smoke is bounded: min(1800 s, the time left to LD_HARD); a timeout is ABORT-SMOKE with the reason (a hung smoke must not eat the session's budget)
    smkto=$(( LD_HARD - $(date +%s) )); [ "$smkto" -le 1800 ] || smkto=1800
    if [ "$smkto" -lt 1 ]; then touch "$dir/ABORT-SMOKE"; era "ABORT-SMOKE stage $name: no time left before LD_HARD (the smoke runs for min(1800 s, the time left))"; return 1; fi
    timeout "$smkto" sudo -n env "${SENV[@]}" LD_TAG=smoke LD_SMOKE=1 bash ./$runner
    sbrc=$?
    if [ "$sbrc" = "124" ]; then touch "$dir/ABORT-SMOKE"; era "ABORT-SMOKE stage $name: the smoke timed out after ${smkto}s (the timeout is min(1800 s, the time left to LD_HARD))"; return 1; fi
    if [ "$name" = "t" ]; then python3 ./s67_score.py "${smk[@]}" "$dir/smoke.log" > "$dir/smoke-check.txt" 2>&1
    elif [ -n "${w:-}" ]; then python3 ./s67_score.py "${smk[@]}" "$dir/smoke.log" > "$dir/smoke-check.txt" 2>&1
    else python3 ./s67_score.py "${smk[@]}" "$dir/smoke.log" > "$dir/smoke-check.txt" 2>&1; fi
    smrc=$?
    if [ "$name" = "r2" ] && [ "$smrc" = 0 ]; then python3 ./plan67.py kept-acct "$dir/smoke.log" >> "$dir/smoke-check.txt" 2>&1; smrc=$?; fi
    if [ "$sbrc" != "0" ] || [ "$smrc" != "0" ]; then
      cat "$dir/smoke-check.txt" 2>/dev/null; touch "$dir/ABORT-SMOKE"
      era "ABORT-SMOKE stage $name (smoke-check.txt)"; return 1
    fi
    cat "$dir/smoke-check.txt"; touch "$dir/SMOKE-PASS" "$dir/REHEARSAL-PASS"; era "LD-ALL stage $name SMOKE-PASS (rehearsal: smoke only, no battery)"
    return 2
  fi
  bto=$(( LD_HARD - $(date +%s) ))
  if [ "$bto" -lt 1800 ]; then era "ABORT-BUDGET stage $name: ${bto}s left before LD_HARD (need 1800): not started"; touch "$dir/NOT-STARTED"; return 8; fi
  T1=$(date +%s)
  timeout "$bto" sudo -n env "${SENV[@]}" LD_TAG=ld bash ./$runner
  brc=$?
  era "LD-ALL stage $name battery rc=$brc wall=$(( $(date +%s) - T1 ))s"
  if [ "$brc" = "124" ]; then      # timeout's rc: the battery was killed at LD_HARD: every scorer reads the line and prints TRUNCATED in place of a PASS / CLEAR
    { echo; echo "LD-BATTERY-TRUNCATED at LD_HARD $(date +%s)"; } >> "$dir/ld.log"; touch "$dir/TRUNCATED"; era "TRUNCATED stage $name: the battery hit LD_HARD (LD-BATTERY-TRUNCATED in $dir/ld.log)"
  fi
  if [ "$brc" = "6" ]; then touch "$dir/FAILED-$name"; era "FAILED-$name stage $name stopped by the block-1 rule (the next stage still runs)"; return 6; fi
  if [ "$brc" = "0" ] && [ -n "${w:-}" ]; then
    # plan 41d's replacement rule (status 47): a slot voided by a witness (not DNF, not crash, never an S06-FAIL) is re-run once, at most 4
    python3 ./place38_parse.py replacements41c "$dir/ld.log" > "$dir/repl-slots.txt" 2> "$dir/repl-slots.err"
    local rrc=$?
    if [ "$rrc" = "3" ]; then era "LD-ALL stage $name: TOO-MANY voided slots, no replacement runs; the guard reads NO VERDICT ($(cat "$dir/repl-slots.err"))"
    elif [ "$rrc" = "0" ] && [ -s "$dir/repl-slots.txt" ]; then
      bto=$(( LD_HARD - $(date +%s) ))
      if [ "$bto" -ge 900 ]; then
        era "LD-ALL stage $name: re-running $(wc -l < "$dir/repl-slots.txt") voided slot(s) once: $(tr '\n' ';' < "$dir/repl-slots.txt")"
        timeout "$bto" sudo -n env "${SENV[@]}" LD_REPL_FILE="$dir/repl-slots.txt" LD_TAG=repl bash ./$runner
        rrc=$?
        era "LD-ALL stage $name: replacement run rc=$rrc"
        if [ "$rrc" = "124" ]; then { echo; echo "LD-BATTERY-TRUNCATED at LD_HARD $(date +%s)"; } >> "$dir/repl.log"; touch "$dir/TRUNCATED"; era "TRUNCATED stage $name: the replacement run hit LD_HARD (LD-BATTERY-TRUNCATED in $dir/repl.log)"; fi
      else era "ABORT-BUDGET stage $name replacement: ${bto}s left before LD_HARD (need 900)"; fi
    fi
  fi
  if [ -n "${w:-}" ]; then python3 ./s67_score.py "${scr[@]}" "$dir/ld.log" "$dir/repl.log" > "$dir/score.txt" 2>&1
  else python3 ./s67_score.py "${scr[@]}" "$dir/ld.log" > "$dir/score.txt" 2>&1; fi
  if [ -n "${w:-}" ]; then
    nrows=$(grep -ac '^P38ROW ' "$dir/ld.log")
    [ "$brc" = "0" ] && [ "$nrows" = "$G_ROWS67" ] && ok=1
    [ "$nrows" = "$G_ROWS67" ] || era "ABORT-ROWS stage $name: $nrows P38ROW lines, the guard's design is $G_ROWS67"
  elif [ "$name" = "t" ]; then
    [ "$brc" = "0" ] && grep -aq '^T67DONE' "$dir/ld.log" && ! grep -aq '^TUN-BRINGUP-FAIL' "$dir/ld.log" && ok=1      # a bring-up failure is ABORT-BRINGUP-T (registration 5.4): the stage is not ok
  else
    [ "$brc" = "0" ] && python3 ./plan67.py check "$bplan" "$dir/ld.log" && ok=1
  fi
  era "LD-ALL stage $name end ok=$ok $(grep -aE '^(VERDICT|GATE-67|R2-HOLD|A1|AS |R1|T )' "$dir/score.txt" | head -3 | tr '\n' ';' | cut -c1-200)"
  [ -z "${w:-}" ] || era "LD-ALL stage $name resolution NOT computed on the VM; on the host: python3 tools/l1/place38_parse.py resolution41d <pulled run dir>/$name/ld.log"
  [ -e "$dir/TRUNCATED" ] && return 7      # cut at LD_HARD: scored (TRUNCATED verdicts), not a failure
  [ "$ok" = "1" ] && touch "$dir/DONE" && return 0
  return 1
}

# ── 8. the session ──
P67RC=()
if [ "$REHEARSE" = 1 ]; then
  for _st in "${P67STAGES[@]}"; do
    stage67 "$_st"; _rc=$?
    [ "$_rc" = "2" ] || fail_all "rehearsal: stage $_st smoke did not pass ($_st)"
  done
  {
    echo "AMENDMENT2 plan 67 rehearsal $(date -u +%FT%TZ)"
    for _s in "${P67NEED[@]}"; do echo "BINSHA $_s ${P67SHA[$_s]} commit=${P67PIN[$_s]}"; done
    if grep -aq 'DUMP-TWINS-NEEDED' "$RUN/sa/smoke-check.txt" 2>/dev/null; then echo "DUMP67=1 (the [SUCC] line lacks the close times without RWM_SUCC_DUMP: run the dump twins, LD_DUMP67=1)"; else echo "DUMP67=0 (the [SUCC] line carries both close times: no dump twins)"; fi
    echo "SUITE-REUSE HIT for: $(for _s in S1 S2; do [[ " ${P67NEED[*]} " == *" $_s "* ]] && echo -n "$_s "; done)"
    echo "TOOLS-MANIFEST $(sha256sum "$SD/TOOLS-MANIFEST.sha256" 2>/dev/null | cut -d' ' -f1)"
  } > "$RUN/AMENDMENT2.txt"
  cat "$RUN/AMENDMENT2.txt"
  persist; echo "LD-ALL-REHEARSAL-DONE"; exit 0
fi
P67BAD=(); P67CUT=()
for _st in "${P67STAGES[@]}"; do
  stage67 "$_st"; _rc=$?
  case "$_rc" in 0) ;; 7|8) P67CUT+=("$_st(rc=$_rc)") ;; *) P67BAD+=("$_st(rc=$_rc)") ;; esac
done
{
  echo "VM BATCH 1 (status 67, plan $PLAN): suites/TESTS.txt, one score.txt per stage"
  for _st in "${P67STAGES[@]}"; do
    _d="$RUN/$(python3 -c "import plan67; print(plan67.STAGE_DIR['$_st'])")"
    echo "== $_st"; grep -aE '^(VERDICT|GATE-67|A1|AS |R1|R2-HOLD|T |FAILED|S06-FAIL|NO VERDICT|UNSCOREABLE)' "$_d/score.txt" 2>/dev/null; ls "$_d" 2>/dev/null | grep -a '^FAILED-' | sed 's/^/MARKER /'
  done
} > "$RUN/score.txt"
era "LD-ALL end plan $PLAN stages=${P67STAGES[*]} not-ok=${P67BAD[*]:-none} cut-at-LD_HARD=${P67CUT[*]:-none} raptorpath=$(pgrep -xc raptorpath) netns=$(sudo -n ip netns list 2>/dev/null | grep -c '^rp-')"
if [ "${#P67BAD[@]}" = 0 ] && [ "${#P67CUT[@]}" = 0 ]; then touch "$RUN/DONE-ALL"; persist; echo "LD-ALL-DONE"; exit 0; fi
# A session cut at LD_HARD with no stage failing is TRUNCATED-ALL: a distinct outcome, never a pass (its verdicts read TRUNCATED) and not a failure (exit 7). Any other not-ok stage is FAILED-ALL.
if [ "${#P67BAD[@]}" = 0 ]; then era "LD-ALL TRUNCATED: plan $PLAN cut at LD_HARD: ${P67CUT[*]} (verdicts read TRUNCATED; not a pass, not a failure)"; touch "$RUN/TRUNCATED-ALL"; persist; echo "LD-ALL-TRUNCATED"; exit 7; fi
fail_all "plan $PLAN: stages not run to the end: ${P67BAD[*]}${P67CUT[*]:+; cut at LD_HARD: ${P67CUT[*]}}"
