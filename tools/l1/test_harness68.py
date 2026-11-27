#!/usr/bin/env python3
"""Tests for status 68's harness (plan 68, the hand-off gate's step 1: tools/l1/plan68.py and the place38_parse / battery / run_all wiring). The end-to-end launch paths run in test_stubrun.py (case68); the
[HOFF] parser and the report are tested in test_s68.py.

  1. the registered table: 54 rows, 18 per arm, 9 per cell, 3 reps per (cell, arm), seed 42, interleaved (no arm owns a position, no arm runs twice in a row within a cell block);
  2. the executed table: the UNBUILT cell (BU) is dropped, 45 rows (LR and PC are built since the pacer, 4ee1df9b, is restored); the smoke / rehearsal is one row per arm;
  3. the per-arm run spec: R4 and GATE share ONE binary seat and differ in RWM_HOFF_GATE alone; BASE sets neither RWM_QUIC_CC nor the gate; RWM_DIAG=1 on every row; Realtime cells set rho = 1;
  4. the paced cells are specified (rate, bytes, timeout) and built; the bursty cell stays UNBUILT; the built paced cells arm the pacer preflight;
  5. the shell side: plan accepted, the two seats, the per-arm scrub of the gate, the budget, the pacer preflight;
  6. the rerun table 68r: 36 rows (GATE and the gate-alone arm Q on K2 / K8 / A2, B / R4 / GATE on LR / PC), interleaved, its smoke, the Q arm's env, the wiring of LD_PLAN=68r.
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
for letter in plan68.ARMS_68:
    check(f"18 rows of arm {plan68.ARM_NAME[letter]} ({letter}*)", sum(a[0] == letter for a in arms) == 18, sum(a[0] == letter for a in arms))
check("9 rows per cell code, 3 per (cell, arm), seed 42 and reps 1..3 only",
      all(sum(plan68.arm_parse(a)[1] == c for a in arms) == 9 for c in plan68.REGISTERED_CELLS)
      and all(arms.count(plan68.arm_name(l, c)) == 3 for l in plan68.ARMS_68 for c in plan68.REGISTERED_CELLS) and {r[3] for r in reg} == {"42"} and {r[4] for r in reg} == {1, 2, 3})
check("every slot is unique (cell, arm, seed, rep) and valid for its arm", len({(r[0], r[2], r[3], r[4]) for r in reg}) == 54 and all(plan68.valid_slot(r[0], r[2]) for r in reg))
check("the topology column is c2-100 or c8-100 only (K8 alone is c8)", {r[0] for r in reg} == {"c2-100", "c8-100"} and {plan68.arm_parse(r[2])[1] for r in reg if r[0] == "c8-100"} == {"K8"})
# interleaved: within one cell block the three arms of a rep are all different and their order rotates across reps; no arm sits in the same position in all three reps of a cell
ok_il = True
for code in plan68.REGISTERED_CELLS:
    pos = {l: [] for l in plan68.ARMS_68}
    for rep in (1, 2, 3):
        blk = [r[2][0] for r in reg if plan68.arm_parse(r[2])[1] == code and r[4] == rep]
        ok_il &= sorted(blk) == sorted(plan68.ARMS_68)
        for i, l in enumerate(blk):
            pos[l].append(i)
    ok_il &= all(len(set(v)) == 3 for v in pos.values())
check("interleaved: each cell's rep block holds the three arms once, and every arm takes each of the three positions once across the reps", ok_il)
check("rep-major: all of rep 1 runs before any of rep 2 (a drift in time spreads over the arms)", [r[4] for r in reg] == sorted(r[4] for r in reg))

# ── 2. the executed table ──
ex = plan68.plan("68")
check("UNBUILT = BU only (the paced cells have their pacer back; the bursty cell has no source)", plan68.UNBUILT == ("BU",) and plan68.BUILT == ("K2", "K8", "A2", "LR", "PC"), (plan68.UNBUILT, plan68.BUILT))
check("the executed table is the registered table minus the UNBUILT cell: 45 rows, in the registered order", len(ex) == 45 and ex == [r for r in reg if plan68.arm_parse(r[2])[1] in plan68.BUILT], len(ex))
sm = plan68.plan("68", smoke=True)
check("the smoke and the rehearsal are one row per arm (B, R, G) on K2 at seed 42 rep 1", [r[2] for r in sm] == ["BK2", "RK2", "GK2"] and sm == plan68.plan("68", rehearsal=True) and {r[3] for r in sm} == {"42"}, sm)
check("the battery's row-count assertion is 54 - 9 x |UNBUILT| and equals len(plan)", 54 - 9 * len(plan68.UNBUILT) == len(ex) == plan68.expect_rows())

# ── 3. the run spec ──
env = {a: dict(x.split("=", 1) for x in plan68.arm_env("c2-100", a)) for a in ("BK2", "RK2", "GK2", "GA2")}
check("BASE sets neither the controller override nor the gate; R4 sets bbr_rs and NOT the gate; GATE sets both", "RWM_QUIC_CC" not in env["BK2"] and "RWM_HOFF_GATE" not in env["BK2"]
      and env["RK2"].get("RWM_QUIC_CC") == "bbr_rs" and "RWM_HOFF_GATE" not in env["RK2"] and env["GK2"].get("RWM_QUIC_CC") == "bbr_rs" and env["GK2"].get("RWM_HOFF_GATE") == "1", env)
check("R4 and GATE differ in RWM_HOFF_GATE alone (one binary, one environment arm wide)", {k: v for k, v in env["GK2"].items() if k != "RWM_HOFF_GATE"} == env["RK2"])
check("RWM_DIAG=1 on every arm of every cell (so [HOFF] prints)", all(dict(x.split("=", 1) for x in plan68.arm_env(plan68.CELLS[c][0], plan68.arm_name(l, c))).get("RWM_DIAG") == "1" for c in plan68.CELL_ORDER for l in plan68.arms_of(c)))
check("seat: B runs the BASE binary, R and G the CAND binary (the one binary of R4 and GATE)", [plan68.seat_of(a) for a in ("BK2", "RK2", "GK2")] == ["BASE", "CAND", "CAND"])
check("hint, bytes and timeout: K2 / K8 Bulk 600 MB at 200 s, A2 Auto 600 MB at 200 s", [plan68.arm_hint(a) for a in ("GK2", "GK8", "GA2")] == ["bulk", "bulk", "auto"] and plan68.arm_bytes("c2-100", "GK2") == 600_000_000
      and plan68.arm_timeout("c2-100", "GK2") == 200 and plan68.arm_bytes("c8-100", "GK8") == 600_000_000)
check("Realtime cells (LR, BU) set rho = 1 (RWM_C_RELIABLE=1: the witness is window_reliable: true on both ends); the Bulk / Auto cells set no retention knob",
      "RWM_C_RELIABLE=1" in plan68.arm_env("c2-100", "GLR") and "RWM_C_RELIABLE=1" in plan68.arm_env("c2-100", "GBU") and all("RWM_C_RELIABLE=1" not in plan68.arm_env("c2-100", a) for a in ("GK2", "GA2", "GPC")))

# ── 4. the paced cells ──
check("the low-rate cell is fully specified: 36 datagrams/s x 494 B = 0.142272 Mbit/s over 60 s, Realtime, timeout 75 s", plan68.rate_token("GLR") == "0.142272" and plan68.arm_bytes("c2-100", "GLR") == 36 * 60 * 494
      and plan68.arm_timeout("c2-100", "GLR") == 75 and plan68.arm_hint("GLR") == "realtime" and "RWM_PERF_RATE_MBIT=0.142272" in plan68.arm_env("c2-100", "GLR"))
check("the paced-headroom cell is fully specified: 70 % of the 100 Mbit/s link, Auto, 60 s (525 MB)", plan68.rate_token("GPC") == "70" and plan68.arm_bytes("c2-100", "GPC") == 525_000_000 and plan68.arm_hint("GPC") == "auto" and plan68.arm_timeout("c2-100", "GPC") == 75)
def _armspec_under(plan, arm):
    os.environ["LD_PLAN"] = plan
    try:
        return plan68.main(["armspec", "c2-100", arm])
    finally:
        os.environ["LD_PLAN"] = "68"


check("the bursty cell is specified (10 datagrams / 33 ms x 494 B x 1818 frames over 60 s, Realtime, rho = 1, timeout 75 s, RWM_PERF_BURST=10/33 and no rate); plans 68 / 68r refuse it at armspec, 68s accepts it",
      plan68.burst_token("GBU") == "10/33" and plan68.rate_token("GBU") is None and plan68.arm_bytes("c2-100", "GBU") == 1818 * 10 * 494 and plan68.arm_timeout("c2-100", "GBU") == 75 and plan68.arm_hint("GBU") == "realtime"
      and "RWM_PERF_BURST=10/33" in plan68.arm_env("c2-100", "GBU") and "RWM_PERF_RATE_MBIT" not in " ".join(plan68.arm_env("c2-100", "GBU")) and _armspec_under("68", "GBU") == 1 and _armspec_under("68r", "GBU") == 1
      and _armspec_under("68s", "GBU") == 0)
check("armspec refuses every UNBUILT cell's arms and accepts the built ones (Q only on the K cells)", all(plan68.main(["armspec", plan68.CELLS[c][0], plan68.arm_name(l, c)]) == 1 for c in plan68.UNBUILT for l in plan68.ARMS_68)
      and all(plan68.main(["armspec", plan68.CELLS[c][0], plan68.arm_name(l, c)]) == 0 for c in plan68.BUILT for l in plan68.arms_of(c)) and plan68.main(["armspec", "c2-100", "QLR"]) == 1)
check("the built paced cells (LR, PC) arm the pacer preflight (needs_pacer); the burst-knob preflight is armed by plan 68s alone (needs_burst)", plan68.needs_pacer() and plan68.needs_burst("68s") and not plan68.needs_burst("68") and not plan68.needs_burst("68r"))
check("row_problems: a wrong seat binary is a problem, a right one is none; a paced row without the pacing echo is a problem",
      plan68.row_problems("c2-100", "GK2", [], [], "x" * 64, "x" * 64, "y" * 64) == [] and plan68.row_problems("c2-100", "BK2", [], [], "x" * 64, "x" * 64, "y" * 64) != []
      and "pace-echo-missing" in plan68.row_problems("c2-100", "GLR", [], [], "x" * 64, "x" * 64, "y" * 64) and plan68.row_problems("c2-100", "GLR", ["offered-load pacing ACTIVE"], [], "x" * 64, "x" * 64, "y" * 64) == []
      and "burst-echo-missing" in plan68.row_problems("c2-100", "GBU", ["offered-load pacing ACTIVE (RWM_PERF_RATE_MBIT=5)"], [], "x" * 64, "x" * 64, "y" * 64)
      and plan68.row_problems("c2-100", "GBU", ["offered-load pacing ACTIVE (RWM_PERF_BURST=10/33)"], [], "x" * 64, "x" * 64, "y" * 64) == [])

# ── 5. the shell side ──
ra, bt, pp = rd("loopdiag_run_all.sh"), rd("loopdiag_battery.sh"), rd("place38_parse.py")
check("run_all and the battery accept LD_PLAN=68, 68r and 68s; the battery shares plan 63's two-seat shape (P68 sets P63) and keeps the bring-up retry", '{ [ "$PLAN" = "68" ] || [ "$PLAN" = "68r" ] || [ "$PLAN" = "68s" ]; } && { P68=1; P63=1; P38=1; PCAND=1; }' in bt and "68r) P_SMOKE=smoke68" in ra and "68s) P_SMOKE=smoke68" in ra and "|43b|68|68r|68s|46|" in bt and "TRIES=1" not in [ln for ln in bt.splitlines() if "P68=1" in ln][0])
check("the gate is scrubbed from the shell on every plan-68 row, so R4 never inherits it (RWM_QUIC_CC and RWM_C_RELIABLE already are, for every plan)", 'SCRUB_EXTRA+=(-u RWM_HOFF_GATE -u RWM_PERF_BURST)' in bt and "-u RWM_QUIC_CC" in bt and "-u RWM_C_RELIABLE" in bt)
check("the BASE seat's pin has no default at plan 68 (REQUIRED on a launch, RECORDED by the rehearsal); plan 63's CONTROL default is unchanged", "ABORT-BIN-SHA plan 68 needs LD_BIN_SHA_CONTROL" in ra and "8013a312" in ra)
check("the budget guard knows plans 68 (worst case 11 400 s, rehearsal 3 000 s), 68r (8 000 s, rehearsal 3 600 s) and 68s (7 400 s, rehearsal 3 600 s) and plan68.BUDGET_S says the same", "P57_NEED=11400" in ra and "P57_NEED=3000" in ra and "P57_NEED=8000" in ra and "P57_NEED=3600" in ra and "P57_NEED=7400" in ra
      and plan68.BUDGET_S == {"68": 11400, "68-rehearsal": 3000, "68r": 8000, "68r-rehearsal": 3600, "68s": 7400, "68s-rehearsal": 3600})
check("the preflight asks the CAND for the gate arm and [HOFF], the BASE for [HOFF], and both for the pacer echo when a paced cell is built", 'preflight_binary "$BIN" "RWM_HOFF_GATE" "RWM_QUIC_CC"' in bt and '"[HOFF] t="' in bt and "ABORT-PACER plan 68" in bt)
check("the row parser registers every plan-68 arm: B* on the BASE family, R* / G* on the CAND family, hints from the cell", "plan68.all_arms()" in pp and 'AFAM.setdefault(_a68, "BASE" if _a68.startswith("B") else "CAND")' in pp)
check("the new files are LF only and in both crlf_guard lists, and lib.sh forwards the pacer knob for the day a paced cell is built", all(b"\r" not in open(os.path.join(HERE, f), "rb").read() for f in ("plan68.py", "s68_report.py", "test_s68.py", "test_harness68.py"))
      and "crlf_guard plan68.py s68_report.py test_harness68.py test_s68.py" in ra and "crlf_guard plan68.py s68_report.py test_harness68.py test_s68.py" in bt and "RWM_PERF_RATE_MBIT" in rd("lib.sh"))
check("HARNESS_PATHS carries plan 68 with its rehearsal and its launch", {"rehearse", "full"} <= set(hp.HARNESS_PATHS.get("68", ())), hp.HARNESS_PATHS.get("68"))
os.environ["LD_PLAN"] = "68"; os.environ.pop("LD_BASE_SHA", None)
import place38_parse as _pp68
check("plan 68's BASE seat (main + the gauge commit) is a promoted BASE: its wire gauges are not a WITNESS-FAIL (the rehearsal's BK2 row)", _pp68.base_promoted() is True)
# ── 6. the rerun table 68r ──
rr = plan68.plan("68r")
rra = [r[2] for r in rr]
check("68r holds 36 rows: GATE and Q on K2 / K8 / A2 (3 each per cell = 18) and B / R4 / GATE on LR and PC (3 each per cell = 18); no BASE or R4 row on the K cells, no Q row off them",
      len(rr) == 36 and all(rra.count(plan68.arm_name(l, c)) == 3 for c in ("K2", "K8", "A2") for l in "GQ") and all(rra.count(plan68.arm_name(l, c)) == 3 for c in ("LR", "PC") for l in "BRG")
      and not any(plan68.arm_parse(a)[1] in ("K2", "K8", "A2") and a[0] in "BR" for a in rra) and not any(a[0] == "Q" and plan68.arm_parse(a)[1] in ("LR", "PC", "BU") for a in rra) and not any(plan68.arm_parse(a)[1] == "BU" for a in rra), len(rr))
check("68r: 9 GATE rows on the K cells, 9 Q rows, seed 42, reps 1..3, every slot unique and valid", sum(a[0] == "G" and a[1:] in ("K2", "K8", "A2") for a in rra) == 9 and sum(a[0] == "Q" for a in rra) == 9 and {r[3] for r in rr} == {"42"}
      and {r[4] for r in rr} == {1, 2, 3} and len({(r[0], r[2], r[3], r[4]) for r in rr}) == 36 and all(plan68.valid_slot(r[0], r[2]) for r in rr))
ok_rr = [r[4] for r in rr] == sorted(r[4] for r in rr)
for code in ("K2", "K8", "A2", "LR", "PC"):
    letters = ("G", "Q") if code in plan68.K_CELLS else plan68.ARMS_68
    pos = {l: [] for l in letters}
    for rep in (1, 2, 3):
        blk = [r[2][0] for r in rr if plan68.arm_parse(r[2])[1] == code and r[4] == rep]
        ok_rr &= sorted(blk) == sorted(letters)
        for i, l in enumerate(blk):
            pos[l].append(i)
    ok_rr &= all(len(set(v)) == len(letters) if len(letters) == 3 else len(set(v)) == 2 for v in pos.values())
check("68r keeps the interleave rules: rep-major, every cell block holds each of its arms once per rep, and the arm order rotates by (rep - 1 + cell index) so no arm owns a position", ok_rr)
check("68r's smoke is six rows (GK2, QK2, BLR, GLR, BPC, GPC) and equals its rehearsal; the battery reads the same tables", [r[2] for r in plan68.plan("68r", smoke=True)] == list(plan68.SMOKE_68R) and plan68.plan("68r", smoke=True) == plan68.plan("68r", rehearsal=True)
      and plan68.expect_rows("68r") == 36 and plan68.expect_rows("68") == 45)
qe = dict(x.split("=", 1) for x in plan68.arm_env("c2-100", "QK2"))
check("the Q arm (gate alone) is the CAND binary with the gate and NO controller override, so quinn's own BBR runs; K8 / A2 likewise; armspec accepts it",
      qe == {"RWM_HOFF_GATE": "1", "RWM_DIAG": "1"} and plan68.seat_of("QK8") == "CAND" and plan68.arm_hint("QA2") == "auto" and plan68.main(["armspec", "c8-100", "QK8"]) == 0 and plan68.arm_bytes("c8-100", "QK8") == 600_000_000)
check("the BASE seat of the paced rows is the same LD_BIN_CONTROL as in 68 (the pacer-carrying BASE binary): BLR / BPC run the BASE seat and carry the pacing rate", plan68.seat_of("BLR") == "BASE" and "RWM_PERF_RATE_MBIT=0.142272" in plan68.arm_env("c2-100", "BLR") and "RWM_PERF_RATE_MBIT=70" in plan68.arm_env("c2-100", "BPC"))
check("the shell wires 68r: the battery's plan table (36 rows, 6 smoke rows, the LR / PC drop when UNBUILT) and run_all's stage rep68r, its smoke / report under LD_PLAN=68r",
      'P68R=1' in bt and 'python3 ./plan68.py plan "$PLAN"' in bt and "want=36; unit=9;" in bt and 'want=3; [ "$P68R" = 1 ] && want=6; [ "$P68S" = 1 ] && want=6' in bt and "68) stage rep68 68; rrc=$? ;; 68r) stage rep68r 68r; rrc=$? ;;" in ra
      and 'LD_PLAN="$splan" python3 ./s68_report.py report' in ra and 'LD_PLAN="$splan" python3 ./s68_report.py smoke' in ra and '"68r"' in pp)
# ── 7. the second rerun table 68s ──
rs = plan68.plan("68s")
rsa = [r[2] for r in rs]
check("68s holds 30 rows: GATE on K2 / K8 / A2 (3 each = 9; no BASE, R4 or Q row on the K cells), B / R4 / GATE on BU (9), B / R4 / GATE and the GATE twin T on PH (12); seed 42, reps 1..3, every slot unique and valid",
      len(rs) == 30 and all(rsa.count(plan68.arm_name("G", c)) == 3 for c in ("K2", "K8", "A2")) and not any(plan68.arm_parse(a)[1] in plan68.K_CELLS and a[0] != "G" for a in rsa)
      and all(rsa.count(plan68.arm_name(l, "BU")) == 3 for l in "BRG") and all(rsa.count(plan68.arm_name(l, "PH")) == 3 for l in "BRGT") and not any(plan68.arm_parse(a)[1] in ("LR", "PC") for a in rsa)
      and {r[3] for r in rs} == {"42"} and {r[4] for r in rs} == {1, 2, 3} and len({(r[0], r[2], r[3], r[4]) for r in rs}) == 30 and all(plan68.valid_slot(r[0], r[2]) for r in rs), len(rs))
ok_rs = [r[4] for r in rs] == sorted(r[4] for r in rs)
for code in ("BU", "PH"):
    letters = plan68.arms_of(code)
    pos = {l: [] for l in letters}
    for rep in (1, 2, 3):
        blk = [r[2][0] for r in rs if plan68.arm_parse(r[2])[1] == code and r[4] == rep]
        ok_rs &= sorted(blk) == sorted(letters)
        for i, l in enumerate(blk):
            pos[l].append(i)
    ok_rs &= all(len(set(v)) == 3 for v in pos.values())
check("68s keeps the interleave rules: rep-major, every cell block holds each of its arms once per rep, and the arm order rotates by (rep - 1 + cell index) so no arm of BU / PH sits at one position in all three reps", ok_rs)
check("68s's smoke is six rows (GK2, BBU, GBU, BPH, GPH, TPH) and equals its rehearsal; 68 and 68r keep their 45 / 36 rows", [r[2] for r in plan68.plan("68s", smoke=True)] == list(plan68.SMOKE_68S)
      and plan68.plan("68s", smoke=True) == plan68.plan("68s", rehearsal=True) and plan68.expect_rows("68s") == 30 and plan68.expect_rows("68") == 45 and plan68.expect_rows("68r") == 36)
ph = {a: dict(x.split("=", 1) for x in plan68.arm_env("c2-100", a)) for a in ("BPH", "RPH", "GPH", "TPH")}
check("PH is paced headroom at 85 % of the link (85 Mbit/s, Auto, 637.5 MB, 75 s); T is the A/A twin of GATE: the same seat and env under its own letter, and exists on PH only; BU's B arm is the BASE seat",
      plan68.rate_token("GPH") == "85" and plan68.arm_bytes("c2-100", "GPH") == 637_500_000 and plan68.arm_hint("GPH") == "auto" and plan68.arm_timeout("c2-100", "GPH") == 75 and ph["TPH"] == ph["GPH"]
      and plan68.seat_of("TPH") == plan68.seat_of("GPH") == "CAND" and plan68.seat_of("BBU") == "BASE" and ph["BPH"].get("RWM_PERF_RATE_MBIT") == "85" and "RWM_HOFF_GATE" not in ph["RPH"]
      and plan68.arm_parse("TK2") == (None, None) and plan68.arm_parse("TBU") == (None, None))
check("PC of plans 68 / 68r stays at 70 % (68s adds PH, it does not move PC), and PH is in no registered slot", plan68.rate_token("GPC") == "70" and "PH" not in plan68.REGISTERED_CELLS and not any(plan68.arm_parse(r[2])[1] == "PH" for r in plan68.registered()))
check("the shell wires 68s: the battery's table (30 rows, 6 smoke rows), the burst-knob preflight and scrub, run_all's stage rep68s and its budget",
      'P68S=1' in bt and "want=30;" in bt and 'needs-burst "$PLAN"' in bt and "68s) stage rep68s 68s; rrc=$? ;;" in ra and '"68s"' in pp and '[ "$PLAN" = "68s" ] && { P57_NEED=7400;' in ra)
os.environ["LD_PLAN"] = "68s"
check("plan68.plan_name() follows LD_PLAN (68s) so the report's row count and smoke slots are the second rerun's, and no cell is UNBUILT in it", plan68.plan_name() == "68s" and plan68.expect_rows() == 30 and plan68.unbuilt_in() == () and "BU" in plan68.built_in())
os.environ["LD_PLAN"] = "68r"
check("plan68.plan_name() follows LD_PLAN (68r) so the report's row count and smoke slots are the rerun's", plan68.plan_name() == "68r" and plan68.expect_rows() == 36)
os.environ["LD_PLAN"] = "68"
print("FAILED %d" % fails if fails else "ALL OK")
sys.exit(1 if fails else 0)
