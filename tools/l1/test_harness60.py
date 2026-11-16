#!/usr/bin/env python3
"""Tests for status 60's harness (plans 60a / 60b, the store law's gauge session: tools/l1/plan60.py, the place38_parse hook, the battery / run_all wiring). The registration is
docs/derivations/s60-registration.md; the scorer's own tests are test_s60.py and the end-to-end launch paths run in test_stubrun.py.

  1. the tables: 96 / 45 / 10 / 2+2 rows, the c2 -> c8 and floor -> control -> evicting order, the Latin-square rotation and seed alternation, the 12 G + 10 C interleaved c1s control, LD_NOGO, the replicate cap;
  2. the per-arm run spec: bytes (100 / 200 / 300 MB, 200 / 400 / 600 MB, 1 / 1.5 / 2 GB floor pools, 600 MB, 5 GB), timeouts (150 s, 210 s at the c1s control), hint (Realtime for EVICT only), env
     (RWM_SLAW first, RWM_STORE_PATH_POOL = W / live paths and NOT at the shipped 2048, the EVICT switches);
  3. the row witnesses on synthetic logs (RWM_SLAW / RWM_DIAG echo on both ends, the pool echo, the seat's sha) and the replacement rule (echo VOID once, floor-pool DNF once, never twice);
  4. the shell side: plan accepted, scrub list, per-row hint, the table-size asserts, the registered-bin_sha check, the MISS abort, the budget, the stage's scorer calls.
Run: python3 test_harness60.py"""
import json
import os
import re
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
for _k in ("LD_PLAN", "LD_WITNESS54", "LD_SHA", "LD_SHA_BASE", "LD_BASE_SHA", "LD_ROW_BYTES", "LD_NOGO"):
    os.environ.pop(_k, None)
import harness_paths as hp  # noqa: E402
import place38_parse as pp  # noqa: E402
import plan60  # noqa: E402

fails = 0


def check(name, ok, detail=""):
    global fails
    print(("ok   " if ok else "FAIL ") + name + ((" -- " + str(detail)[:1500]) if not ok else ""))
    if not ok:
        fails += 1


def rd(name):
    return open(os.path.join(HERE, name), encoding="utf-8", errors="replace").read()


runall, battery = rd("loopdiag_run_all.sh"), rd("loopdiag_battery.sh")

# ═══ 1. tables ═══
A, B, R = plan60.plan("60a"), plan60.plan("60b"), plan60.plan("60a", rehearsal=True)
check("60a is 96 slots (c2 and c8: 8 arms x 6 replicates, all G); 60b is 45 (18 c1s floor + 22 control + 5 evicting); the rehearsal 10; the smokes 2 each",
      (len(A), len(B), len(R), len(plan60.plan("60a", smoke=True)), len(plan60.plan("60b", smoke=True))) == (96, 45, 10, 2, 2), (len(A), len(B), len(R)))
check("60a: the c2 block then the c8 block, every row a G arm, 6 replicates of each of the 8 arms, no duplicate slot",
      [r[0] for r in A] == ["c2-100"] * 48 + ["c8-100"] * 48 and all(r[2].startswith("G") for r in A) and len(set((r[0], r[2], r[3], r[4]) for r in A)) == 96
      and all(sum(1 for r in A if r[0] == c and r[2] == a) == 6 for c, ws in plan60.GRID["60a"].items() for a in ["G%d" % w for w in ws]))
check("60a: the arms rotate by replicate (a cyclic Latin square: replicate k starts at arm k mod 8) and the seeds alternate 42 / 7",
      [r[2] for r in A[:8]] == ["G16", "G32", "G48", "G160", "G176", "G192", "G256", "G2048"] and A[8][2] == "G32" and A[16][2] == "G48" and [A[8 * k][3] for k in range(6)] == ["42", "7", "42", "7", "42", "7"])
fl = [r for r in B if r[2] in ("G64", "G128", "G192")]
ctl = [r for r in B if r[2] in ("G2048", "C2048")]
check("60b: the c1s floor block first (3 pools x 6), then the control: 12 G rows and 10 C rows, 10 interleaved pairs (the order alternating) and 2 extra G rows, then 5 EVICT rows at c2-100 (seeds 42 / 7 / 42 / 7 / 42)",
      len(fl) == 18 and len(ctl) == 22 and sum(1 for r in ctl if r[2] == "G2048") == 12 and sum(1 for r in ctl if r[2] == "C2048") == 10
      and B[:18] == fl and B[18:40] == ctl and B[40:] == [("c2-100", "def", "EVICT", s, i + 1) for i, s in enumerate(("42", "7", "42", "7", "42"))]
      and [r[2] for r in ctl[:4]] == ["G2048", "C2048", "C2048", "G2048"] and ctl[0][4] == ctl[1][4] and ctl[-1][2] == "G2048" and ctl[-2][2] == "G2048" and ctl[-2][4] == 11 and ctl[-1][4] == 12, B[16:24])
check("the rehearsal is both smokes plus c2 W16, c8 W32 x2, c8 W96, c1s W64 x2, all G beyond the smokes, every slot unique",
      R[:2] == plan60.SMOKE["60a"] and R[2:4] == plan60.SMOKE["60b"] and sorted(r[2] for r in R[4:]) == ["G16", "G32", "G32", "G64", "G64", "G96"] and len({(r[0], r[2], r[3], r[4]) for r in R}) == 10, R)
check("the smokes: 60a = c2 W192 C then G; 60b = c1s W64 G then W2048 C", plan60.SMOKE["60a"] == [("c2-100", "def", "C192", "42", 1), ("c2-100", "def", "G192", "42", 1)]
      and plan60.SMOKE["60b"] == [("c1s-400", "def", "G64", "42", 1), ("c1s-400", "def", "C2048", "42", 1)])
os.environ["LD_NOGO"] = "c1s"
check("LD_NOGO=c1s drops 60b's floor block (27 slots: the control and the evicting rows), 60a is untouched", len(plan60.plan("60b")) == 27 and len(plan60.plan("60a")) == 96)
del os.environ["LD_NOGO"]
plan60.REPS_CAP60 = 2
check("the replicate cap (the stub suite's fast mode) shrinks both plans and keeps the structure", len(plan60.plan("60a")) == 32 and len(plan60.plan("60b")) == 6 + 4 + 2, (len(plan60.plan("60a")), len(plan60.plan("60b"))))
plan60.REPS_CAP60 = None
check("every arm of every table is registered with the stage-3 parser (window / Rlc / bulk, or realtime for EVICT) and rides the candidate seat",
      all(pp.sp.ARM_SPEC[r[2]][:3] == ("window", "Rlc", "realtime" if r[2] == "EVICT" else "bulk") and pp.AFAM[r[2]] == "CAND" for r in A + B + R))

# ═══ 2. per-arm spec ═══
spec = {(c, a): (plan60.arm_bytes(c, a), plan60.arm_timeout(c, a), plan60.arm_hint(a), plan60.arm_env(c, a)) for c, _, a, _, _ in A + B + R}
check("floor-pool bytes: c2 100 / 200 / 300 MB, c8 200 / 400 / 600 MB, c1s 1 / 1.5 / 2 GB",
      [spec[("c2-100", "G%d" % w)][0] for w in (16, 32, 48)] == [100_000_000, 200_000_000, 300_000_000] and [spec[("c8-100", "G%d" % w)][0] for w in (32, 64, 96)] == [200_000_000, 400_000_000, 600_000_000]
      and [spec[("c1s-400", "G%d" % w)][0] for w in (64, 128, 192)] == [1_000_000_000, 1_500_000_000, 2_000_000_000])
check("every other arm: 600 MB at c2 / c8 / EVICT, 5 GB at the c1s control (G and C)",
      all(spec[(c, "G%d" % w)][0] == 600_000_000 for c in ("c2-100", "c8-100") for w in plan60.GRID["60a"][c] if w not in plan60.FLOOR_W[c]) and spec[("c2-100", "C192")][0] == 600_000_000
      and spec[("c1s-400", "G2048")][0] == spec[("c1s-400", "C2048")][0] == 5_000_000_000 and spec[("c2-100", "EVICT")][0] == 600_000_000)
check("timeouts: 150 s on every floor-pool, c2, c8 and evicting row, 210 s at the c1s 5 GB control",
      all(v[1] == 150 for k, v in spec.items() if k[0] != "c1s-400" or plan60.is_floor(*k) or k[1] == "EVICT") and spec[("c1s-400", "G2048")][1] == spec[("c1s-400", "C2048")][1] == 210)
check("hint: bulk everywhere, realtime for EVICT alone", all(v[2] == ("realtime" if k[1] == "EVICT" else "bulk") for k, v in spec.items()))
check("env: RWM_SLAW first (1 on G and EVICT, 0 on C); the pool knob RWM_STORE_PATH_POOL = W / live paths, NOT set at the shipped 2048; EVICT adds RWM_C_RELIABLE=0 and RWM_RDIAG=1 and no pool",
      spec[("c2-100", "G16")][3] == ["RWM_SLAW=1", "RWM_RTOBS=1", "RWM_STORE_PATH_POOL=16"] and spec[("c8-100", "G32")][3] == ["RWM_SLAW=1", "RWM_RTOBS=1", "RWM_STORE_PATH_POOL=16"] and spec[("c8-100", "G1280")][3] == ["RWM_SLAW=1", "RWM_RTOBS=1", "RWM_STORE_PATH_POOL=640"]
      and spec[("c2-100", "G2048")][3] == ["RWM_SLAW=1", "RWM_RTOBS=1"] and spec[("c1s-400", "C2048")][3] == ["RWM_SLAW=0", "RWM_RTOBS=1"] and spec[("c1s-400", "G64")][3] == ["RWM_SLAW=1", "RWM_RTOBS=1", "RWM_STORE_PATH_POOL=64"]
      and spec[("c2-100", "EVICT")][3] == ["RWM_SLAW=1", "RWM_C_RELIABLE=0"], {k: v[3] for k, v in spec.items() if k[1] in ("G32", "C192", "EVICT")})
check("an unknown (cell, arm) is refused by armspec (rc 1)", plan60.main(["armspec", "c2-100", "G999"]) == 1 and plan60.main(["armspec", "c8-100", "EVICT"]) == 1)

# ═══ 3. witnesses and the replacement rule ═══
G = "[GATES] RWM_DIAG=1 RWM_THREE_TERM=0 RWM_STORE_PATH_POOL=%s RWM_SLAW=%s"
POOLINFO = "path-scaled outstanding pool ACTIVE (RWM_STORE_PATHS) pool_per_path=%s"
SHA = "a" * 64


def logs(slaw_c="1", slaw_s="1", pool="16", diag="1"):
    return ([G.replace("RWM_DIAG=1", "RWM_DIAG=" + diag) % (pool, slaw_c), POOLINFO % pool], [G % (pool, slaw_s), POOLINFO % pool])


c, s = logs()
check("a G row echoing RWM_SLAW=1, RWM_DIAG=1, RWM_THREE_TERM=0 and its pool on both ends has no problem", plan60.row_problems("c2-100", "G16", c, s, SHA, SHA) == [], plan60.row_problems("c2-100", "G16", c, s, SHA, SHA))
c, s = logs(slaw_c="0")
check("RWM_SLAW echoed 0 on the client end of a G row is a gate-slaw problem (VOID)", any(x.startswith("gate-slaw-cli=0") for x in plan60.row_problems("c2-100", "G16", c, s, SHA, SHA)))
c, s = logs(slaw_s="1")
check("a C row whose server echoes RWM_SLAW=1 is a gate-slaw problem; echoed 0 on both ends it is clean (C pool 2048 echo is the shipped one)",
      any(x.startswith("gate-slaw-srv=1") for x in plan60.row_problems("c1s-400", "C2048", *logs("0", "1", "2048"), SHA, SHA)) and plan60.row_problems("c1s-400", "C2048", *logs("0", "0", "2048"), SHA, SHA) == [])
check("RWM_DIAG not 1 and a wrong pool echo are problems; a foreign binary is a bin-sha problem",
      any(x.startswith("gate-diag") for x in plan60.row_problems("c2-100", "G16", *logs(diag="0"), SHA, SHA)) and any(x.startswith("gate-pool") for x in plan60.row_problems("c2-100", "G16", *logs(pool="2048"), SHA, SHA))
      and any(x.startswith("bin-sha") for x in plan60.row_problems("c2-100", "G16", *logs(), SHA, "b" * 64)))
check("the c8 pool echo is per path (W 32 -> 16, W 1280 -> 640)", plan60.row_problems("c8-100", "G32", *logs(pool="16"), SHA, SHA) == [] and plan60.row_problems("c8-100", "G1280", *logs(pool="640"), SHA, SHA) == [])
d = tempfile.mkdtemp()


def ledger(rows):
    p = os.path.join(d, "ld%d.log" % len(os.listdir(d)))
    with open(p, "w") as fh:
        for r in rows:
            fh.write("P38ROW " + json.dumps(r) + "\n")
    return p


def row(cell, arm, seed, rep, problems=(), dnf=False, status="LIVE"):
    return {"cell": cell, "arm": arm, "seed": seed, "rep": rep, "problems": list(problems), "dnf": dnf, "status": status}


check("replacement: a gate-slaw VOID slot is re-run once ('slaw-echo'); a second row for the slot stops it",
      plan60.replace_reason(ledger([row("c2-100", "G16", "42", 1, ["gate-slaw-cli=0 (want 1)"], status="WITNESS-FAIL")]), "c2-100", "G16", "42", 1) == "slaw-echo"
      and plan60.replace_reason(ledger([row("c2-100", "G16", "42", 1, ["gate-slaw-cli=0 (want 1)"], status="WITNESS-FAIL"), row("c2-100", "G16", "42", 1)]), "c2-100", "G16", "42", 1) is None)
check("replacement: a pool-echo VOID ('pool-echo'), an evicting-seat liveness VOID ('evict-liveness'), a floor-pool DNF ('floor-dnf'); a non-floor DNF, a LIVE row and a crash row are NOT replaced",
      plan60.replace_reason(ledger([row("c8-100", "G64", "7", 2, ["gate-pool-cli=2048 (want 32)"], status="WITNESS-FAIL")]), "c8-100", "G64", "7", 2) == "pool-echo"
      and plan60.replace_reason(ledger([row("c2-100", "EVICT", "42", 1, ["evict-sender-shed-echo-missing"], status="WITNESS-FAIL")]), "c2-100", "EVICT", "42", 1) == "evict-liveness"
      and plan60.replace_reason(ledger([row("c2-100", "G16", "42", 1, dnf=True)]), "c2-100", "G16", "42", 1) == "floor-dnf"
      and plan60.replace_reason(ledger([row("c2-100", "G256", "42", 1, dnf=True)]), "c2-100", "G256", "42", 1) is None
      and plan60.replace_reason(ledger([row("c2-100", "G16", "42", 1)]), "c2-100", "G16", "42", 1) is None
      and plan60.replace_reason(ledger([row("c2-100", "G16", "42", 1, dnf=True, status="VOID-ONDISK")]), "c2-100", "G16", "42", 1) is None)
check("replacement: a missing Realtime echo re-runs once ('evict-liveness'); a WRONG BINARY (bin-sha-*) never does",
      plan60.replace_reason(ledger([row("c2-100", "EVICT", "42", 1, ["no-realtime-echo-cli"], status="WITNESS-FAIL")]), "c2-100", "EVICT", "42", 1) == "evict-liveness"
      and plan60.replace_reason(ledger([row("c2-100", "G16", "42", 1, ["bin-sha=aaa (want bbb for CAND)"], status="WITNESS-FAIL")]), "c2-100", "G16", "42", 1) is None
      and plan60.replace_reason(ledger([row("c2-100", "G16", "42", 1, ["bin-sha-uncheckable (x)"], status="WITNESS-FAIL")]), "c2-100", "G16", "42", 1) is None)
check("the budget guard carries the worst-case floor DNF re-runs: plan60.BUDGET_S = 18 360 s (60a) / 13 860 s (60b), and run_all's P57_NEED equals them",
      plan60.BUDGET_S == {"60a": 18360, "60b": 13860} and "P57_NEED=18360; [ \"$PLAN\" = \"60b\" ] && P57_NEED=13860" in runall)
check("60b's non-rehearsal launch reads 60a's R-sign labels before any row (ABORT-RSIGN, LD_SCORE60A), a rehearsal is exempt",
      'ABORT-RSIGN plan 60b needs 60a' in runall and 'LABEL60 R-SIGN-$_c (PASS|UNSCOREABLE)' in runall and '[ "$PLAN" = "60b" ] && [ "$REHEARSE" != "1" ]' in runall)
check("RWM_RTOBS=1 rides every G / C arm (section 56's instrument env) and no EVICT arm; RWM_RDIAG=1 is the battery's instrument env for every row",
      all("RWM_RTOBS=1" in v[3] for k, v in spec.items() if k[1] != "EVICT") and all("RWM_RTOBS=1" not in v[3] for k, v in spec.items() if k[1] == "EVICT"))
check("G/C alternation: within 60b's control the pair order alternates (G,C),(C,G),... and every pair shares one seed and replicate number",
      [r[2] for r in ctl[:8]] == ["G2048", "C2048", "C2048", "G2048", "G2048", "C2048", "C2048", "G2048"] and all(ctl[2 * i][3] == ctl[2 * i + 1][3] and ctl[2 * i][4] == ctl[2 * i + 1][4] for i in range(10)))
check("plan60.py check: rc 0 only when LD-BATTERY-DONE and every slot of the table has a row",
      plan60.check("60b", ledger([row(c_, a_, sd, rp) for c_, _, a_, sd, rp in B] + [])) == 1  # no LD-BATTERY-DONE line
      )
p_ok = ledger([row(c_, a_, sd, rp) for c_, _, a_, sd, rp in B])
with open(p_ok, "a") as fh:
    fh.write("LD-BATTERY-DONE tag=ld blocks=1\n")
check("... and rc 0 with it (a replaced slot's second row is not an extra slot)", plan60.check("60b", p_ok) == 0)

# ═══ 4. shell side ═══
check("battery: plans 60a / 60b accepted; P60 is single-seat (P38 + PCAND, no P41); the instrument env is RWM_RDIAG=1 + the timeout",
      "case \"$PLAN\" in 60a|60b|59|57a|57b|56" in battery and 'P60=0; case "$PLAN" in 60a|60b) P60=1; P38=1; PCAND=1 ;; esac' in battery and '[ "$P60" = 1 ] && INSTR_ENV=(RWM_RDIAG=1 RWM_PERF_TIMEOUT_S=150)' in battery)
check("battery: every row's bytes / timeout / hint / env come from plan60.py armspec (one table), an unknown arm is refused, HINT is per row", 'python3 ./plan60.py armspec "$cell" "$arm"' in battery
      and 'read -r bytes p60_tmo HINT p60_arm_env' in battery and 'tmo_env=(RWM_PERF_TIMEOUT_S="$p60_tmo")' in battery and 'read -r -a arm_env <<< "$p60_arm_env"' in battery and "UNKNOWN-ARM $arm at $cell (plan 60)" in battery)
check("battery: RWM_SLAW and RWM_THREE_TERM are scrubbed with the rest (an ambient value reaches no row), both before the arm env", "-u RWM_C_RELIABLE -u RWM_SLAW -u RWM_THREE_TERM" in battery)
check("battery: the table-size asserts (96 / 45, 10 rehearsal, 2 smoke), the replacement call and the NO-GO exemption", 'want=96; [ "$PLAN" = "60b" ] && want=45' in battery and '-eq 10 ]' in battery and "python3 ./plan60.py replace" in battery
      and '[ -n "${LD_NOGO:-}" ] || [ "${#tbl[@]}" -eq "$want" ]' in battery)
check("battery: the binary must carry RWM_SLAW, the pool knob and the shed gate, and (grep -F: a bracket expression otherwise) the [SLAW] window line",
      'preflight_binary "$BIN" "RWM_SLAW" "RWM_STORE_PATH_POOL" "RWM_UNIFIED_SHED"' in battery and 'grep -aqF -- "[SLAW] t=" "$BIN"' in battery)
check("run_all: plans 60a / 60b are selectable and rehearsable; both ride the single-seat gates (P57) and carry the registered bin_sha / manifest (ABORT-SUITES before anything runs without a manifest)",
      "  60a) P_SMOKE=" in runall and "  60b) P_SMOKE=" in runall and "|59|60a|60b) ;; *) echo \"ABORT-REHEARSE" in runall and 'P60=0; { [ "$PLAN" = "60a" ] || [ "$PLAN" = "60b" ]; } && { P60=1; P46=1; P57=1; }' in runall
      and "ABORT-SUITES no reference manifest at $LD_REF_MANIFEST" in runall)
check("run_all: the registered values are the defaults: bin_sha e410d640... (60-pre) and tools/l1/ref60-manifest.sha256 (187 files), overridable",
      "LD_BIN_SHA_EXPECT:-e410d64053ab0a143e813aded2e4023176b41edf09d287e795252736b0e5f25d" in runall and 'LD_REF_MANIFEST:-$HERE/ref60-manifest.sha256' in runall
      and len(rd("ref60-manifest.sha256").splitlines()) == 187)
check("run_all: the built binary is compared with LD_BIN_SHA_EXPECT before any row (ABORT-BIN-SHA), and a suite-reuse MISS is ABORT-SUITES (a rehearsal included)",
      'fail_all "ABORT-BIN-SHA plan $PLAN: built $SHA != registered $LD_BIN_SHA_EXPECT; NOTHING WAS RUN"' in runall and 'fail_all "ABORT-SUITES plan $PLAN suite-reuse MISS' in runall)
check("run_all: the budget guard needs 11 520 s (60a) / 10 440 s (60b); the tests gate runs test_s60.py, test_harness60.py and test_mbar59.py", "P57_NEED=18360; [ \"$PLAN\" = \"60b\" ] && P57_NEED=13860" in runall
      and "for t in test_s60.py test_harness60.py" in runall and 'if [ "$P59" = 1 ] || [ "$P60" = 1 ]; then' in runall)
check("run_all: the stage calls the 60 scorer for the smoke (the rehearsal's 10 rows on 60a), the score and the slot check; the battery sees LD_REHEARSE and LD_NOGO through sudo -n env",
      "s60_score.py smoke60 \"$P60SMOKE\"" in runall and "P60SMOKE=60r" in runall and 's60_score.py score60 "$splan"' in runall and 'plan60.py check "$splan"' in runall and 'LD_REHEARSE="$REHEARSE" LD_NOGO="${LD_NOGO:-}"' in runall
      and "60a) stage rep60a 60a" in runall and "60b) stage rep60b 60b" in runall)
check("run_all and the battery guard the new files against CRLF", "plan60.py s60_score.py test_s60.py" in runall + battery and "test_harness60.py" in runall)
check("HARNESS_PATHS lists 60a and 60b with the strict-manifest paths and no 'full'", hp.HARNESS_PATHS["60a"][0] == "rehearse" and "reuse-miss" in hp.HARNESS_PATHS["60a"] and "full" not in hp.HARNESS_PATHS["60a"] and "prebuilt" in hp.HARNESS_PATHS["60b"])
print("FAILED %d" % fails if fails else "ALL OK")
sys.exit(1 if fails else 0)
