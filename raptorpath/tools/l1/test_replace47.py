#!/usr/bin/env python3
"""Tests for the plan-41c replacement rule (status 47): a slot voided by a witness (not DNF, not crash) is re-run once at the end of the battery with
the same (cell, arm, seed, rep); the replacement takes the slot; at most 4 replacements per session, beyond that NO VERDICT; DNF and crash rows are
never replaced (they take the worst rank). Also the c2 / c8 parse path of plan 41c and the default seed of resolution41c.
Run: python3 test_replace47.py   (no multiprocessing is used here)"""
import json
import os
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
os.environ.pop("LD_PLAN", None)
import guard_power as gp  # noqa: E402
import l1common as lc  # noqa: E402
import place38_parse as pp  # noqa: E402
import place41b_score as g  # noqa: E402

fails = 0


def check(name, ok, detail=""):
    global fails
    print(("ok   " if ok else "FAIL ") + name + ((" -- " + str(detail)) if not ok else ""))
    if not ok:
        fails += 1


bat = open(os.path.join(HERE, "loopdiag_battery.sh"), encoding="utf-8").read()
runall = open(os.path.join(HERE, "loopdiag_run_all.sh"), encoding="utf-8").read()


def ledger():
    return gp.dry_ledger(5)


def slot_of(rows, cell, arm, seed, rep):
    return next(i for i, r in enumerate(rows) if (r["cell"], r["arm"], r["seed"], int(r["rep"])) == (cell, arm, seed, rep))


def void(rows, cell, arm, seed, rep, status="WITNESS-FAIL"):
    i = slot_of(rows, cell, arm, seed, rep)
    rows[i]["status"] = status
    rows[i]["problems"] = ["void for the test"]


def replacement(rows, cell, arm, seed, rep, status="LIVE"):
    r = dict(rows[slot_of(rows, cell, arm, seed, rep)])
    r["status"], r["problems"] = status, [] if status == "LIVE" else ["void again"]
    return r


def score_text(rows):
    lines = []
    with tempfile.TemporaryDirectory() as td:
        pth = os.path.join(td, "l.log")
        open(pth, "w").write("".join("P38ROW " + json.dumps(r) + chr(10) for r in rows))
        pp.score41c([pth], out=lines.append)
    return lines


def verdict(lines):
    return next((ln for ln in lines if ln.startswith("VERDICT ")), None)


base = ledger()
ok_v = verdict(score_text(base))
check("sanity: the complete 96-row dry ledger gives a verdict line (GATE-41c PASS or FAIL)", ok_v is not None and ok_v.startswith("VERDICT GATE-41c "), ok_v)

# 1. a voided slot with no replacement: NO VERDICT (as before)
a = ledger()
void(a, "c8-100", "CAND", "7", 3)
check("a slot voided by a witness and not replaced reads NO VERDICT, re-run", verdict(score_text(a)).startswith("VERDICT NO VERDICT, re-run"), verdict(score_text(a)))

# 2. the replacement row (appended at the end, same slot) takes the slot
b = ledger()
void(b, "c8-100", "CAND", "7", 3)
b.append(replacement(b, "c8-100", "CAND", "7", 3))
lb = score_text(b)
check("a replacement row for the voided slot (same cell, arm, seed, rep) takes the slot: a verdict is printed", verdict(lb).startswith("VERDICT GATE-41c "), verdict(lb))
check("... the scorer prints which slots were replaced, with the original and replacement statuses",
      any(ln.startswith("REPLACED SLOTS") and "1 of at most 4" in ln for ln in lb) and any("c8-100 CAND s7 r3" in ln and "WITNESS-FAIL -> LIVE" in ln for ln in lb), [ln for ln in lb if "REPLACED" in ln or "c8-100 CAND s7 r3" in ln][:3])
check("... and the ROWS line still counts 96 slots, naming the extra ledger row", any(ln.startswith("ROWS 96/96") and "1 replacement row" in ln for ln in lb), lb[1])


# 3. replacement also void
c = ledger()
void(c, "c8-100", "CAND", "7", 3)
c.append(replacement(c, "c8-100", "CAND", "7", 3, status="WITNESS-FAIL"))
check("a replacement that is itself voided leaves the slot empty: NO VERDICT", verdict(score_text(c)).startswith("VERDICT NO VERDICT, re-run"))

# 4. DNF is never replaced: it takes the worst rank, and a later row for a LIVE or DNF slot is ignored (and said)
d = ledger()
i = slot_of(d, "c1s-400", "CAND", "42", 2)
d[i]["dnf"], d[i]["mbps"] = True, None
d.append(dict(d[i], dnf=False, mbps=800.0))                      # an unexpected later row for the DNF slot
ld = score_text(d)
check("a DNF slot is not replaced: the DNF row stays, ranked worst, and the later row is ignored and named",
      any("c1s-400 CAND s42 r2 DNF" in ln for ln in ld if "worst-ranked" in ln) and any("IGNORED EXTRA ROW" in ln and "c1s-400 CAND s42 r2" in ln for ln in ld) and not any(ln.startswith("REPLACED SLOTS") and " 1 of" in ln for ln in ld), [ln for ln in ld if "worst-ranked" in ln or "IGNORED" in ln])

# 5. crash rows (VOID-RC / NO_DATA) are never replaced and take the worst rank (they fill the slot)
e = ledger()
for rep in range(1, 7):
    for sd in ("42", "7"):
        void(e, "c8-100", "CAND", sd, rep, status="VOID-RC")
le = score_text(e)
check("a crashing CAND build (all 12 CAND rows at c8 VOID-RC) is a FAIL, not a NO VERDICT: crash rows fill their slots and take the worst rank on every clause",
      verdict(le) == "VERDICT GATE-41c FAIL", verdict(le))
check("... and the 12 crash rows are listed as worst-ranked", any("worst-ranked" in ln and ln.count("CRASH") == 12 for ln in le), [ln for ln in le if "worst-ranked" in ln][:1])
e1 = ledger()
void(e1, "c2-100", "BASE", "42", 4, status="NO_DATA")
check("one NO_DATA row is not replaced and does not stop the verdict (worst rank in its slot)", verdict(score_text(e1)).startswith("VERDICT GATE-41c "))

# 6. at most 4 replacements per session
f4 = ledger()
slots4 = [("c1s-400", "BASE", "42", 1), ("c1d-400", "CAND", "7", 2), ("c2-100", "BASE", "42", 3), ("c8-100", "CAND", "7", 4)]
for s_ in slots4:
    void(f4, *s_)
for s_ in slots4:
    f4.append(replacement(f4, *s_))
l4 = score_text(f4)
check("exactly 4 voided slots, all replaced: a verdict is printed (4 is the cap)", verdict(l4).startswith("VERDICT GATE-41c ") and any(ln.startswith("REPLACED SLOTS") and "4 of at most 4" in ln for ln in l4), verdict(l4))
f5 = ledger()
slots5 = slots4 + [("c8-100", "BASE", "7", 5)]
for s_ in slots5:
    void(f5, *s_)
for s_ in slots5:
    f5.append(replacement(f5, *s_))
l5 = score_text(f5)
check("5 voided slots is beyond the cap: NO VERDICT even though every slot has a replacement row, and the reason is printed",
      verdict(l5).startswith("VERDICT NO VERDICT, re-run") and any("more than 4" in ln for ln in l5), (verdict(l5), [ln for ln in l5 if "more than 4" in ln][:1]))

# 7. the replacement list (what the battery re-runs)
def slots_cmd(rows):
    with tempfile.TemporaryDirectory() as td:
        pth = os.path.join(td, "ld.log")
        open(pth, "w").write("".join("P38ROW " + json.dumps(r) + chr(10) for r in rows))
        cp = subprocess.run([sys.executable, os.path.join(HERE, "place38_parse.py"), "replacements41c", pth], capture_output=True, text=True)
    return cp


mixed = ledger()
void(mixed, "c1s-400", "CAND", "42", 1, status="CONTAMINATED")
void(mixed, "c1d-400", "BASE", "7", 2, status="VOID-COTENANT")
void(mixed, "c2-100", "CAND", "7", 3, status="VOID-RC")                # a crash: never replaced
i = slot_of(mixed, "c8-100", "BASE", "42", 4)
mixed[i]["dnf"] = True                                                 # a DNF: never replaced
cp = slots_cmd(mixed)
check("`replacements41c` lists exactly the slots voided by a witness (CONTAMINATED, VOID-COTENANT here), in ledger order, as `cell store arm seed rep`; crash and DNF slots are not listed",
      cp.returncode == 0 and cp.stdout.splitlines() == ["c1s-400 def CAND 42 1", "c1d-400 def BASE 7 2"], (cp.returncode, cp.stdout, cp.stderr[-200:]))
five = ledger()
for s_ in slots5:
    void(five, *s_)
cp5 = slots_cmd(five)
check("`replacements41c` with 5 voided slots prints nothing, says TOO-MANY on stderr and exits 3 (no replacement runs: NO VERDICT)", cp5.returncode == 3 and cp5.stdout == "" and "TOO-MANY 5" in cp5.stderr, (cp5.returncode, cp5.stderr[-120:]))
check("`replacements41c` on a clean ledger prints nothing and exits 0", slots_cmd(ledger()).returncode == 0 and slots_cmd(ledger()).stdout == "")

# 8. the shell side
check("battery: plan 41c can run a replacement list (LD_REPL_FILE, at most 4 rows, tag repl) with the same (cell, arm, seed, rep)",
      "LD_REPL_FILE" in bat and "ABORT-PLAN41C replacement list" in bat and '[ "${#tbl[@]}" -ge 1 ] && [ "${#tbl[@]}" -le 4 ]' in bat)
check("run_all guard stage: after the 96 rows it asks `replacements41c`, re-runs the voided slots once into repl.log, and scores ld.log + repl.log; more than 4 -> no runs, NO VERDICT",
      'replacements41c "$dir/ld.log"' in runall and 'LD_REPL_FILE=' in runall and "LD_TAG=repl" in runall and '"$scr" "$dir/ld.log" "$dir/repl.log"' in runall and "TOO-MANY" in runall)

# 9. resolution41c: the default seed is named and printed
lines = []
saved = (g.RES_DRAWS, g.RES_GRID)
g.RES_DRAWS, g.RES_GRID = 1, [0.05]
pp.resolution41c(["/nonexistent/ld.log"], out=lines.append)
g.RES_DRAWS, g.RES_GRID = saved
check("resolution41c: the default seed is named (RES_SEED_DEFAULT = 0) and printed in the header ('seed 0 (default)')", g.RES_SEED_DEFAULT == 0 and "seed 0 (default)" in lines[0], lines[:1])
lines = []
pp.resolution41c(["/nonexistent/ld.log"], out=lines.append, seed=3)
check("... an explicit seed is printed as such", "seed 3 (given)" in lines[0] and "default" not in lines[0].split("seed 3")[1][:12], lines[:1])

# 10. plan 41c at c2 and c8: the same row parser and the same setup as 38's c2 / c8 rows
import stage3_parse as sp  # noqa: E402
c2c8 = [r for r in pp.plan41c() if r[0] in ("c2-100", "c8-100")]
check("plan 41c has 48 c2 / c8 rows (24 each: 12 BASE + 12 CAND), store def", len(c2c8) == 48 and all(r[1] == "def" for r in c2c8))
check("the cell specs of c2-100 and c8-100 are the ones plan 38 uses: c2 c2 single / c2 c3 dual, 100 MB", sp.CELL_SPEC["c2-100"] == ("c2", "c2", "single", 100000000) and sp.CELL_SPEC["c8-100"] == ("c2", "c3", "dual", 100000000))
check("the battery has ONE cell_spec for c2-100 / c8-100 (shared by plans 38 and 41c) and the 41c row path is plan 38's (P38=1: RTOBS + RDIAG env, place38_parse row parser)",
      bat.count('c2-100)  echo "c2 c2 single 100000000"') == 1 and bat.count('c8-100)  echo "c2 c3 dual 100000000"') == 1
      and 'P38=0; { [ "$PLAN" = "38" ] || [ "$PLAN" = "41" ] || [ "$PLAN" = "41b" ] || [ "$PLAN" = "41c" ]; } && P38=1' in bat)
RAW = os.path.join(HERE, "..", "..", "docs", "l1-raw")
PD = os.path.join(RAW, "thread-pd", "battery", "diag-tpd", "c1d-400-MAIN-s42-r1-a1-")
drv, cli, srv = (lc.read(PD + x) for x in ("drv.out", "c.log", "s.log"))
for cell in ("c2-100", "c8-100"):
    for arm in ("BASE", "CAND"):
        r = pp.make_row(cell, arm, "42", 1, 0, 12.0, drv, cli, srv, 0, "x", store="def")
        check(f"a synthetic {cell} {arm} row goes through plan 38's make_row (cell known, N_LEGS {sp.N_LEGS[cell]}, BDP {pp.bdp_bytes(cell)}): status {r['status']}, no cell-specific failure",
              r["cell"] == cell and r["status"] in ("LIVE", "WITNESS-FAIL", "CONTAMINATED") and "qcwnd_below" in r and not any("KeyError" in str(p) for p in r["problems"]), (r["status"], r["problems"]))

print("FAILED %d" % fails if fails else "ALL OK")
sys.exit(1 if fails else 0)
