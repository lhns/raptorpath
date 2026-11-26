#!/usr/bin/env python3
"""Tests for status 68's harness (plan 68, the hand-off gate's step 1: tools/l1/plan68.py and the place38_parse / battery / run_all wiring). The end-to-end launch paths run in test_stubrun.py (case68); the
[HOFF] parser and the report are tested in test_s68.py.

  1. the registered table: 54 rows, 18 per arm, 9 per cell, 3 reps per (cell, arm), seed 42, interleaved (no arm owns a position, no arm runs twice in a row within a cell block);
  2. the executed table: the UNBUILT cells (LR, PC, BU) are dropped, 27 rows; the smoke / rehearsal is one row per arm;
  3. the per-arm run spec: R4 and GATE share ONE binary seat and differ in RWM_HOFF_GATE alone; BASE sets neither RWM_QUIC_CC nor the gate; RWM_DIAG=1 on every row; Realtime cells set rho = 1;
  4. the paced cells are specified (rate, bytes, timeout) but UNBUILT, and the one switch that builds them also arms the pacer preflight;
  5. the shell side: plan accepted, the two seats, the per-arm scrub of the gate, the budget, the pacer preflight.
Run: python3 test_harness68.py"""
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
for _k in ("LD_PLAN", "LD_WITNESS54", "LD_SHA", "LD_SHA_BASE", "LD_BASE_SHA", "LD_ROW_BYTES", "LD_BIN_SHA_CAND", "LD_BIN_SHA_CONTROL", "LD_REHEARSE"):
    os.environ.pop(_k, None)
import harness_paths as hp  # noqa: E402
import plan68  # noqa: E402

fails = 0


def check(name, ok, detail=""):
    global fails
    print(("ok   " if ok else "FAIL ") + name + ((" -- " + str(detail)[:1500]) if not ok else ""))
    if not ok:
        fails += 1


def rd(name):
    return open(os.path.join(HERE, name), encoding="utf-8", errors="replace").read()


# ── 1. the registered table ──
reg = plan68.registered()
arms = [r[2] for r in reg]
check("the registered table holds 54 rows (6 cells x 3 arms x 3 rows)", len(reg) == 54, len(reg))
for letter in plan68.ARM_LETTERS:
    check(f"18 rows of arm {plan68.ARM_NAME[letter]} ({letter}*)", sum(a[0] == letter for a in arms) == 18, sum(a[0] == letter for a in arms))
check("9 rows per cell code, 3 per (cell, arm), seed 42 and reps 1..3 only",
      all(sum(plan68.arm_parse(a)[1] == c for a in arms) == 9 for c in plan68.CELL_ORDER)
      and all(arms.count(plan68.arm_name(l, c)) == 3 for l in plan68.ARM_LETTERS for c in plan68.CELL_ORDER) and {r[3] for r in reg} == {"42"} and {r[4] for r in reg} == {1, 2, 3})
check("every slot is unique (cell, arm, seed, rep) and valid for its arm", len({(r[0], r[2], r[3], r[4]) for r in reg}) == 54 and all(plan68.valid_slot(r[0], r[2]) for r in reg))
check("the topology column is c2-100 or c8-100 only (K8 alone is c8)", {r[0] for r in reg} == {"c2-100", "c8-100"} and {plan68.arm_parse(r[2])[1] for r in reg if r[0] == "c8-100"} == {"K8"})
# interleaved: within one cell block the three arms of a rep are all different and their order rotates across reps; no arm sits in the same position in all three reps of a cell
ok_il = True
for code in plan68.CELL_ORDER:
    pos = {l: [] for l in plan68.ARM_LETTERS}
    for rep in (1, 2, 3):
        blk = [r[2][0] for r in reg if plan68.arm_parse(r[2])[1] == code and r[4] == rep]
        ok_il &= sorted(blk) == sorted(plan68.ARM_LETTERS)
        for i, l in enumerate(blk):
            pos[l].append(i)
    ok_il &= all(len(set(v)) == 3 for v in pos.values())
check("interleaved: each cell's rep block holds the three arms once, and every arm takes each of the three positions once across the reps", ok_il)
check("rep-major: all of rep 1 runs before any of rep 2 (a drift in time spreads over the arms)", [r[4] for r in reg] == sorted(r[4] for r in reg))

# ── 2. the executed table ──
ex = plan68.plan("68")
check("UNBUILT = LR, PC, BU (the paced cells need a pacer neither tree carries; the bursty cell has no source)", plan68.UNBUILT == ("LR", "PC", "BU") and plan68.BUILT == ("K2", "K8", "A2"), (plan68.UNBUILT, plan68.BUILT))
check("the executed table is the registered table minus the UNBUILT cells: 27 rows, in the registered order", len(ex) == 27 and ex == [r for r in reg if plan68.arm_parse(r[2])[1] in plan68.BUILT], len(ex))
sm = plan68.plan("68", smoke=True)
check("the smoke and the rehearsal are one row per arm (B, R, G) on K2 at seed 42 rep 1", [r[2] for r in sm] == ["BK2", "RK2", "GK2"] and sm == plan68.plan("68", rehearsal=True) and {r[3] for r in sm} == {"42"}, sm)
check("the battery's row-count assertion is 54 - 9 x |UNBUILT| and equals len(plan)", 54 - 9 * len(plan68.UNBUILT) == len(ex) == plan68.expect_rows())

# ── 3. the run spec ──
env = {a: dict(x.split("=", 1) for x in plan68.arm_env("c2-100", a)) for a in ("BK2", "RK2", "GK2", "GA2")}
check("BASE sets neither the controller override nor the gate; R4 sets bbr_rs and NOT the gate; GATE sets both", "RWM_QUIC_CC" not in env["BK2"] and "RWM_HOFF_GATE" not in env["BK2"]
      and env["RK2"].get("RWM_QUIC_CC") == "bbr_rs" and "RWM_HOFF_GATE" not in env["RK2"] and env["GK2"].get("RWM_QUIC_CC") == "bbr_rs" and env["GK2"].get("RWM_HOFF_GATE") == "1", env)
check("R4 and GATE differ in RWM_HOFF_GATE alone (one binary, one environment arm wide)", {k: v for k, v in env["GK2"].items() if k != "RWM_HOFF_GATE"} == env["RK2"])
check("RWM_DIAG=1 on every arm of every cell (so [HOFF] prints)", all(dict(x.split("=", 1) for x in plan68.arm_env(plan68.CELLS[c][0], plan68.arm_name(l, c))).get("RWM_DIAG") == "1" for c in plan68.CELL_ORDER for l in plan68.ARM_LETTERS))
check("seat: B runs the BASE binary, R and G the CAND binary (the one binary of R4 and GATE)", [plan68.seat_of(a) for a in ("BK2", "RK2", "GK2")] == ["BASE", "CAND", "CAND"])
check("hint, bytes and timeout: K2 / K8 Bulk 600 MB at 200 s, A2 Auto 600 MB at 200 s", [plan68.arm_hint(a) for a in ("GK2", "GK8", "GA2")] == ["bulk", "bulk", "auto"] and plan68.arm_bytes("c2-100", "GK2") == 600_000_000
      and plan68.arm_timeout("c2-100", "GK2") == 200 and plan68.arm_bytes("c8-100", "GK8") == 600_000_000)
check("Realtime cells (LR, BU) set rho = 1 (RWM_C_RELIABLE=1: the witness is window_reliable: true on both ends); the Bulk / Auto cells set no retention knob",
      "RWM_C_RELIABLE=1" in plan68.arm_env("c2-100", "GLR") and "RWM_C_RELIABLE=1" in plan68.arm_env("c2-100", "GBU") and all("RWM_C_RELIABLE=1" not in plan68.arm_env("c2-100", a) for a in ("GK2", "GA2", "GPC")))

# ── 4. the paced cells ──
check("the low-rate cell is fully specified: 36 datagrams/s x 494 B = 0.142272 Mbit/s over 60 s, Realtime, timeout 75 s", plan68.rate_token("GLR") == "0.142272" and plan68.arm_bytes("c2-100", "GLR") == 36 * 60 * 494
      and plan68.arm_timeout("c2-100", "GLR") == 75 and plan68.arm_hint("GLR") == "realtime" and "RWM_PERF_RATE_MBIT=0.142272" in plan68.arm_env("c2-100", "GLR"))
check("the paced-headroom cell is fully specified: 70 % of the 100 Mbit/s link, Auto, 60 s (525 MB)", plan68.rate_token("GPC") == "70" and plan68.arm_bytes("c2-100", "GPC") == 525_000_000 and plan68.arm_hint("GPC") == "auto" and plan68.arm_timeout("c2-100", "GPC") == 75)
check("the bursty cell has no source: asking for its bytes is an error, the shell's armspec refuses it", plan68.CELLS["BU"][3] is None and plan68.main(["armspec", "c2-100", "GBU"]) == 1)
check("armspec refuses every UNBUILT cell's arms and accepts the built ones", all(plan68.main(["armspec", plan68.CELLS[c][0], plan68.arm_name(l, c)]) == 1 for c in plan68.UNBUILT for l in plan68.ARM_LETTERS)
      and all(plan68.main(["armspec", plan68.CELLS[c][0], plan68.arm_name(l, c)]) == 0 for c in plan68.BUILT for l in plan68.ARM_LETTERS))
check("no built cell needs the pacer today; flipping a paced cell to built turns the pacer preflight on", not plan68.needs_pacer() and (lambda: (setattr(plan68, "BUILT", plan68.BUILT + ("LR",)), plan68.needs_pacer(), setattr(plan68, "BUILT", plan68.BUILT[:-1]))[1])())
check("row_problems: a wrong seat binary is a problem, a right one is none; a paced row without the pacing echo is a problem",
      plan68.row_problems("c2-100", "GK2", [], [], "x" * 64, "x" * 64, "y" * 64) == [] and plan68.row_problems("c2-100", "BK2", [], [], "x" * 64, "x" * 64, "y" * 64) != []
      and "pace-echo-missing" in plan68.row_problems("c2-100", "GLR", [], [], "x" * 64, "x" * 64, "y" * 64) and plan68.row_problems("c2-100", "GLR", ["offered-load pacing ACTIVE"], [], "x" * 64, "x" * 64, "y" * 64) == [])

# ── 5. the shell side ──
ra, bt, pp = rd("loopdiag_run_all.sh"), rd("loopdiag_battery.sh"), rd("place38_parse.py")
check("run_all and the battery accept LD_PLAN=68; the battery shares plan 63's two-seat shape (P68 sets P63) and keeps the bring-up retry", 'P68=0; [ "$PLAN" = "68" ] && { P68=1; P63=1; P38=1; PCAND=1; }' in bt and "68) P_SMOKE=smoke68" in ra and "|43b|68|46|" in bt and "TRIES=1" not in [ln for ln in bt.splitlines() if "P68=1" in ln][0])
check("the gate is scrubbed from the shell on every plan-68 row, so R4 never inherits it (RWM_QUIC_CC and RWM_C_RELIABLE already are, for every plan)", 'SCRUB_EXTRA+=(-u RWM_HOFF_GATE)' in bt and "-u RWM_QUIC_CC" in bt and "-u RWM_C_RELIABLE" in bt)
check("the BASE seat's pin has no default at plan 68 (REQUIRED on a launch, RECORDED by the rehearsal); plan 63's CONTROL default is unchanged", "ABORT-BIN-SHA plan 68 needs LD_BIN_SHA_CONTROL" in ra and "8013a312" in ra)
check("the budget guard knows plan 68 (worst case 11 400 s, rehearsal 3 000 s) and plan68.BUDGET_S says the same", "P57_NEED=11400" in ra and "P57_NEED=3000" in ra and plan68.BUDGET_S == {"68": 11400, "68r": 3000})
check("the preflight asks the CAND for the gate arm and [HOFF], the BASE for [HOFF], and both for the pacer echo when a paced cell is built", 'preflight_binary "$BIN" "RWM_HOFF_GATE" "RWM_QUIC_CC"' in bt and '"[HOFF] t="' in bt and "ABORT-PACER plan 68" in bt)
check("the row parser registers every plan-68 arm: B* on the BASE family, R* / G* on the CAND family, hints from the cell", "plan68.all_arms()" in pp and 'AFAM.setdefault(_a68, "BASE" if _a68.startswith("B") else "CAND")' in pp)
check("the new files are LF only and in both crlf_guard lists, and lib.sh forwards the pacer knob for the day a paced cell is built", all(b"\r" not in open(os.path.join(HERE, f), "rb").read() for f in ("plan68.py", "s68_report.py", "test_s68.py", "test_harness68.py"))
      and "crlf_guard plan68.py s68_report.py test_harness68.py test_s68.py" in ra and "crlf_guard plan68.py s68_report.py test_harness68.py test_s68.py" in bt and "RWM_PERF_RATE_MBIT" in rd("lib.sh"))
check("HARNESS_PATHS carries plan 68 with its rehearsal and its launch", {"rehearse", "full"} <= set(hp.HARNESS_PATHS.get("68", ())), hp.HARNESS_PATHS.get("68"))
print("FAILED %d" % fails if fails else "ALL OK")
sys.exit(1 if fails else 0)
