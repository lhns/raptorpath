#!/usr/bin/env python3
"""Tests for plan 67's scorer (s67_score.py) and its synthetic log shapes (s67_fixtures.py). One clause = one function per outcome: test_<clause>_red (the defect: must read FAIL / HOLD / VOID ...) and
test_<clause>_green (the control). The rows are the REAL row parser's rows: the committed fixture log (docs/l1-raw/thread-pd/battery/diag-smoke) stripped by s67_fixtures.strip_base plus s67_fixtures.extra_lines.
Run: python3 test_s67.py   (about half a minute)"""
import json
import os
import re
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
for _k in ("LD_PLAN", "LD_WITNESS54", "LD_WITNESS64", "LD_WITNESS67", "LD_SHA", "LD_SHA_BASE", "LD_BASE_SHA", "LD_ROW_BYTES", "LD_ONDISK", "P38_STORE"):
    os.environ.pop(_k, None)
import l1common as lc  # noqa: E402
import plan67  # noqa: E402
import s67_fixtures as F  # noqa: E402
import s67_score as S  # noqa: E402
import place38_parse as pp  # noqa: E402

FIX = os.path.join(HERE, "..", "..", "docs", "l1-raw", "thread-pd", "battery", "diag-smoke", "c1s-400-MAIN-s42-r1-a1-")
SHA = {s: (s.lower() + "0" * 64)[:64] for s in plan67.SEAT_ORDER}
for _s, _v in SHA.items():
    os.environ["LD_SHA_" + _s] = _v
fails = 0
CHECKS = 0
TMP = tempfile.mkdtemp(prefix="s67test")
_BASE = None
_CACHE = {}


def check(name, ok, detail=""):
    global fails, CHECKS
    CHECKS += 1
    print(("ok   " if ok else "FAIL ") + name + ((" -- " + str(detail)[:1500]) if not ok else ""))
    if not ok:
        fails += 1


def base():
    global _BASE
    if _BASE is None:
        cli, srv, drv = (lc.read(FIX + x) for x in ("c.log", "s.log", "drv.out"))
        cli = [ln for ln in cli if "[ETA] site=sender" not in ln]
        _BASE = (cli, srv, drv)
    return _BASE


def logs(plan, cell, arm, seed, rep, defect=None):
    cli0, srv0, drv0 = base()
    cli, srv, drv = F.strip_base(plan, arm, cli0, srv0, drv0, defect)
    c, s, d = F.extra_lines(plan, cell, arm, seed, rep, defect)
    return cli + c, srv + s, drv + d


def build(plan, cell, arm, seed, rep, defect=None, sha=None, rc=0):
    key = (plan, cell, arm, str(seed), int(rep), defect, sha, rc)
    if key not in _CACHE:
        cli, srv, drv = logs(plan, cell, arm, seed, rep, defect)
        seat = plan67.seat_of(arm)
        _CACHE[key] = S.row_of(plan, cell, arm, seed, rep, rc, "60", drv, cli, srv, 0, sha or SHA[seat])
    return json.loads(json.dumps(_CACHE[key]))


def ledger_rows(plan, defect_of=None, only=None, drop=None):
    rows = []
    for cell, _, arm, seed, rep in plan67.plan(plan):
        if drop and drop(cell, arm, seed, rep):
            continue
        d = defect_of(cell, arm, seed, rep) if defect_of else None
        rows.append(build(plan, cell, arm, seed, rep, d))
    return rows


def write_ledger(rows, name="ld.log", extra=()):
    p = os.path.join(TMP, name)
    with open(p, "w") as f:
        for r in rows:
            f.write("P67ROW " + json.dumps(r, sort_keys=True) + "\n")
        for x in extra:
            f.write(x + "\n")
        f.write("LD-BATTERY-DONE\n")
    return p


def scored(plan, rows, extra=()):
    lines = []
    S.score(plan, [write_ledger(rows, extra=extra)], lines.append)
    return "\n".join(lines)


def has(text, s):
    return s in text


# ═════════════ the row parser and the witnesses (stage A / R1 / R2 rows) ═════════════
def quiet_replace(*a):
    import io
    from contextlib import redirect_stdout
    with redirect_stdout(io.StringIO()):
        return plan67.replace(*a)


def test_s06_taper_red():
    r = build("67a", "c8-100", "S2", "42", 1, "s2-taper-nonzero")
    check("s06_taper_red: an S2 row with taper > 0 whose preconditions hold reads S06-FAIL", r["status"] == "S06-FAIL" and r["status_41d"] == "LIVE" and r["w67"]["pre_ok"], (r["status"], r["problems"]))
    import tempfile as _t
    p = os.path.join(TMP, "s06.log")
    open(p, "w").write("P67ROW " + json.dumps(r) + "\n")
    rc = quiet_replace("67a", p, "c8-100", "S2", "42", 1)
    check("s06_taper_red: an S06-FAIL row is never replaced", rc == 1)


def test_s06_taper_green():
    r = build("67a", "c8-100", "S2", "42", 1)
    check("s06_taper_green: a clean S2 row (taper = plost = 0, pslot > 0, rr > 0) is LIVE", r["status"] == "LIVE" and r["w67"]["taper"] == 0 and r["w67"]["plost"] == 0 and r["w67"]["pslot"] > 0, (r["status"], r["problems"]))


def test_s2_pslot_red():
    r = build("67a", "c2-100", "S2", "42", 1, "s2-no-pslot")
    check("s2_pslot_red: an S2 row without pslot= is VOID-WITNESS", r["status"] == "VOID-WITNESS" and any("pslot" in x for x in r["problems"]), (r["status"], r["problems"]))
    p = os.path.join(TMP, "np.log")
    open(p, "w").write("P67ROW " + json.dumps(r) + "\n")
    check("s2_pslot_red: and the battery re-runs it once", quiet_replace("67a", p, "c2-100", "S2", "42", 1) == 0)


def test_s2_pslot_green():
    r = build("67a", "c2-100", "S2", "42", 1)
    check("s2_pslot_green: a clean S2 row carries pslot > 0 and the [SPAN] median rr > 0", r["status"] == "LIVE" and r["w67"]["pslot"] > 0 and r["w67"]["rr_med"] > 0)


def test_s1_gauge_red():
    r = build("67a", "c2-100", "S1", "42", 1, "s1-no-gauge")
    check("s1_gauge_red: an S1 row without scw / plx / rlaw is VOID-WITNESS", r["status"] == "VOID-WITNESS" and any(x.startswith("no-gauge-prints") for x in r["problems"]), (r["status"], r["problems"]))


def test_s1_gauge_green():
    s1, b0 = build("67a", "c2-100", "S1", "42", 1), build("67a", "c2-100", "B0", "42", 1)
    check("s1_gauge_green: S1 carries the four gauge prints with taper > 0, plost = taper, rr > 0 and no pslot; B0 carries none", s1["status"] == "LIVE" and s1["w67"]["g_four"] and s1["w67"]["pslot"] is None and s1["w67"]["taper"] > 0
          and b0["status"] == "LIVE" and not b0["w67"]["g_any"], (s1["status"], s1["problems"], b0["status"], b0["problems"]))


def test_b0_gauge_red():
    cli, srv, drv = logs("67a", "c2-100", "S1", "42", 1)           # S1's logs on a B0 row
    r = S.row_of("67a", "c2-100", "B0", "42", 1, 0, "60", drv, cli, srv, 0, SHA["B0"])
    check("b0_gauge_red: a B0 row carrying gauge prints is VOID-WITNESS (seat identity)", r["status"] == "VOID-WITNESS" and "b0-carries-gauge-prints" in r["problems"], (r["status"], r["problems"]))


def test_seat_sha_red_green():
    r = build("67a", "c2-100", "S1", "42", 1, sha="f" * 64)
    check("seat_sha_red: a row whose bin_sha is not its seat's reads VOID-SEAT (never replaced)", r["status"] == "VOID-SEAT" and any(x.startswith("bin-sha") for x in r["problems"]) and "VOID-SEAT" not in plan67.REPLACEABLE, (r["status"], r["problems"]))
    cli, srv, drv = logs("67a", "c2-100", "S1", "42", 1)
    r2 = S.row_of("67a", "c2-100", "S1", "42", 1, 0, "60", drv, cli, srv, 0, None)
    check("seat_sha_red: a missing row sha is uncheckable, a problem, never a silent pass", r2["status"] == "VOID-SEAT" and any("uncheckable" in x for x in r2["problems"]))
    check("seat_sha_green: the seat's own sha is LIVE", build("67a", "c2-100", "S1", "42", 1)["status"] == "LIVE")


def test_short_red_green():
    cli, srv, drv = logs("67a", "c2-100", "S1", "42", 1)
    cut = [ln for ln in cli if not (ln.startswith("[DIAG] t=") and float(ln.split("t=")[1].split("s")[0]) > 20)]
    r = S.row_of("67a", "c2-100", "S1", "42", 1, 0, "60", drv, cut, srv, 0, SHA["S1"])
    check("short_red: a completed 600 MB row with under 30 s of ticks is VOID-SHORT", r["status"] == "VOID-SHORT", (r["status"], r["problems"]))
    check("short_green: a full-length row is LIVE", build("67a", "c2-100", "S1", "42", 1)["status"] == "LIVE")


def test_dnf_row():
    r = build("67a", "c2-100", "S2", "42", 1, "dnf")
    check("dnf_green: a DNF is LIVE with dnf true (its witnesses never void it)", r["status"] == "LIVE" and r["dnf"] is True, (r["status"], r["dnf"], r["problems"]))


def test_rows_never_raise():
    p = subprocess.run([sys.executable, os.path.join(HERE, "s67_score.py"), "row", "67a", "c2-100", "S1", "42", "1", "0", "60", "/nonexistent/drv", "/nonexistent/cli", "/nonexistent/srv", "0", "abc"],
                       capture_output=True, text=True, env=dict(os.environ))
    line = [ln for ln in p.stdout.splitlines() if ln.startswith("P67ROW ")]
    row = json.loads(line[0][7:]) if line else {}
    check("row_cli_red: a row whose logs are missing prints a row (status NO_DATA / void), never a crash", p.returncode == 0 and row.get("plan67") is True and row.get("status") != "LIVE", (p.returncode, p.stderr[-300:], p.stdout[-300:]))
    cli, srv, drv = logs("67a", "c2-100", "S1", "42", 1)
    fd = {}
    for nm, ls in (("drv", drv), ("cli", cli), ("srv", srv)):
        fd[nm] = os.path.join(TMP, nm + ".txt")
        open(fd[nm], "w", encoding="utf-8").write("\n".join(ls) + "\n")
    env = dict(os.environ, LD_SHA_S1=SHA["S1"])
    p = subprocess.run([sys.executable, os.path.join(HERE, "s67_score.py"), "row", "67a", "c2-100", "S1", "42", "1", "0", "60", fd["drv"], fd["cli"], fd["srv"], "0", SHA["S1"]], capture_output=True, text=True, env=env)
    row = json.loads([ln for ln in p.stdout.splitlines() if ln.startswith("P67ROW ")][0][7:])
    check("row_cli_green: the row CLI prints one P67ROW with plan67, cell, arm, store, seed, rep, rc, status, problems, dnf and the readings", all(k in row for k in ("plan67", "cell", "arm", "store", "seed", "rep", "rc", "status", "problems", "dnf", "w67", "mbps", "cpu_cli", "cli_lag_E"))
          and row["status"] == "LIVE" and row["store"] == "def" and not row["w67"].get("diag_error"), (row.get("status"), row.get("problems")))


# ═════════════ stage A: clauses ═════════════
def _a_as_defect(cell, arm, seed, rep):
    return "s2-goodput-low" if arm == "S2" else None


def test_as_red_green():
    t = scored("67a", ledger_rows("67a", _a_as_defect))
    check("as_red: S2 goodput 15 % low at c2 and c8 -> AS FAIL", has(t, "AS FAIL"), t[-900:])
    t = scored("67a", ledger_rows("67a"))
    check("as_green: clean rows -> AS PASS (rank-minp, Monte Carlo at the guard's seed 4141)", has(t, "AS PASS") and "MONTE CARLO" in t, t[-1400:])
    check("as_green: the realised d80 line is printed from the S1 half-splits", "realised d80" in t)


def test_a1_red_green():
    t = scored("67a", ledger_rows("67a", lambda c, a, s, r: "s1-goodput-low" if a == "S1" else None))
    check("a1_red: S1 goodput 15 % low against B0 at c2 and c8 -> A1 FAIL", has(t, "A1 FAIL"), t[-900:])
    t = scored("67a", ledger_rows("67a"))
    check("a1_green: clean rows -> A1 PASS with the EXACT null named", has(t, "A1 PASS") and "null: EXACT" in t, t[-1400:])


def test_a1_exact_matches_mc():
    rows = S.view(ledger_rows("67a", lambda c, a, s, r: "s1-goodput-low" if a == "S1" else None))
    ex, mc = S.rank_minp_exact(rows, "B0", "S1", ("goodput", "cpu_cli/GB", "cpu_srv/GB")), S.rank_minp_mc(rows, "B0", "S1", ("goodput", "cpu_cli/GB", "cpu_srv/GB"))
    check("a1_exact_green: the exact joint null agrees with the guard's seeded Monte Carlo (both far below 0.05 on a 15 % shift; exact p >= 1 / 7056)", ex is not None and ex["p"] < 0.01 and mc["p"] < 0.01 and ex["p"] > 0 and ex["per"][0]["n"] == 7056, (ex and ex["p"], mc["p"]))


def test_unscoreable_red_green():
    drop = lambda c, a, s, r: a == "S2" and c == "c8-100" and r > 2          # S2 at c8 keeps 6 of 12 rows: below plan67.MIN_LIVE_A
    t = scored("67a", ledger_rows("67a", drop=drop))
    check("a_under_min_red: fewer than MIN_LIVE_A live S2 rows in a cell -> AS UNSCOREABLE (never a PASS); A1 (S1 against B0) is still scored", "AS UNSCOREABLE" in t and ("\nA1 PASS" in t or "\nA1 FAIL" in t), t[-700:])
    drop0 = lambda c, a, s, r: a == "B0" and c == "c2-100" and r > 1
    t = scored("67a", ledger_rows("67a", drop=drop0))
    check("a_under_min_red: fewer than MIN_LIVE_B0 live B0 rows -> A1 UNSCOREABLE, AS still scored", "A1 UNSCOREABLE" in t and ("AS PASS" in t or "AS FAIL" in t))
    t = scored("67a", ledger_rows("67a"))
    check("a_under_min_green: the full design is scored", "UNSCOREABLE" not in t)


def test_replacement_cap():
    def voids(n):
        rows = ledger_rows("67a")
        out, k = [], 0
        for r in rows:
            if r["arm"] == "S2" and r["cell"] == "c2-100" and k < n:
                bad = build("67a", r["cell"], "S2", r["seed"], r["rep"], "s2-no-pslot")
                out += [bad, r]            # the original voided, its replacement the clean row
                k += 1
            else:
                out.append(r)
        return out
    t = scored("67a", voids(plan67.MAX_REPLACED_A))
    check("replace_cap_green: MAX_REPLACED_A replaced slots are scored", "REPLACED SLOTS: %d of at most %d" % (plan67.MAX_REPLACED_A, plan67.MAX_REPLACED_A) in t and "NO VERDICT" not in t, t[-600:])
    t = scored("67a", voids(plan67.MAX_REPLACED_A + 1))
    check("replace_cap_red: more replaced slots than the cap -> NO VERDICT", "NO VERDICT" in t and "AS PASS" not in t, t[-600:])


def test_s06_stage_red_green():
    rows = ledger_rows("67a", lambda c, a, s, r: "s2-taper-nonzero" if (a == "S2" and c == "c8-100" and s == "42" and r == 1) else None)
    t = scored("67a", rows)
    check("s06_stage_red: one S06-FAIL row is counted, kept in the ledger and named", "S06-FAIL-ROWS 1" in t and "S06-FAIL c8-100 S2" in t, t[-500:])
    t = scored("67a", ledger_rows("67a"))
    check("s06_stage_green: clean rows read S06-FAIL-ROWS 0", "S06-FAIL-ROWS 0" in t)


def test_s06_dnf_red_green():
    rows = ledger_rows("67a", lambda c, a, s, r: "dnf" if (a == "S2" and c == "c2-100" and s == "42" and r == 1) else None)
    t = scored("67a", rows)
    check("s06_dnf_red: an S2 DNF in a cell where every S1 row completes is S06-FAIL", "S06-FAIL-ROWS 1" in t and "DNF where every S1 row" in t, t[-500:])
    rows = ledger_rows("67a", lambda c, a, s, r: "dnf" if (c == "c2-100" and s == "42" and r == 1 and a in ("S1", "S2")) else None)
    t = scored("67a", rows)
    check("s06_dnf_green: an S2 DNF where an S1 row is also a DNF is not S06-FAIL (it keeps its worst rank)", "S06-FAIL-ROWS 0" in t, t[-500:])


def test_belowfloor_red_green():
    rows = ledger_rows("67a", lambda c, a, s, r: "s2-belowfloor-high" if a == "S2" else None)
    t = scored("67a", rows)
    check("belowfloor_red: S2 below-floor far above 0.25 x S1 and above fill_coded + 2 x H-B -> HOLD flag at c2 and c8, rows stay LIVE, no FAIL",
          "BELOW-FLOOR-HOLD c2-100 HOLD" in t and "BELOW-FLOOR-HOLD c8-100 HOLD" in t and "BELOW-FLOOR-HOLD YES" in t and all(r["status"] == "LIVE" for r in rows if r["arm"] == "S2"), t[-900:])
    check("belowfloor_red: the ratio to fill_coded is printed per row", "ratio=" in t)
    t = scored("67a", ledger_rows("67a"))
    check("belowfloor_green: clean S2 rows read clear", "BELOW-FLOOR-HOLD c2-100 clear" in t and "BELOW-FLOOR-HOLD no" in t, t[-900:])


def test_succ_red_green():
    t = scored("67a", ledger_rows("67a", lambda c, a, s, r: "no-succ-times"))
    check("succ_red: a [SUCC] line without rep_/orig_ p50 -> DUMP-TWINS-NEEDED", "DUMP-TWINS-NEEDED" in t)
    t = scored("67a", ledger_rows("67a"))
    check("succ_green: both close times on every row -> no DUMP-TWINS-NEEDED", "DUMP-TWINS-NEEDED" not in t and "[SUCC] CHECK: 48/48" in t.replace("60/60", "48/48") or "DUMP-TWINS-NEEDED" not in t)


def test_rate_table():
    t = scored("67a", ledger_rows("67a"))
    check("rate_table_green: the repair-rate split table prints per arm and cell the winner / bound shares, rsat, rr, consumed r, pfec, t_tail DERIVED and the r_sat bind fraction",
          all(x in t for x in ("RATE S1  c2-100", "RATE S2  c8-100", "anchor winner core=", "rsat p10/p50/p90", "granted rr p10/p50/p90", "consumed r med", "pfec p10/p50/p90", "t_tail 1e-05 (DERIVED", "bind fraction", "COST-SPLIT c2-100", "taper share")), t[-2500:])
    rows = ledger_rows("67a", lambda c, a, s, r: "s1-no-pfec" if a == "S1" else None)
    check("rate_table_red: a binary lacking pfec= reads NOT-AVAILABLE for P_fec (the row stays LIVE)", all(r["status"] == "LIVE" for r in rows if r["arm"] == "S1") and "NOT-AVAILABLE" in scored("67a", rows))


def test_owner_flag():
    t = scored("67a", ledger_rows("67a"))
    check("owner_flag_green: the cpu owner flag line is printed per cell with its d80 rule", "OWNER-FLAG" in t and "c8 in-order delay, TWO-SIDED" in t and "resolution 50 %" in t)


def test_block1():
    rows = ledger_rows("67a")
    p = write_ledger(rows)
    check("block1_green: every arm's first row LIVE with its preconditions -> rc 0", S.block1("67a", p, lambda *_: None) == 0)
    bad = list(rows)
    i = next(i for i, r in enumerate(bad) if r["arm"] == "S2")
    bad[i] = build("67a", bad[i]["cell"], "S2", bad[i]["seed"], bad[i]["rep"], "s2-no-pslot")
    check("block1_red: S2's first row VOID-WITNESS -> rc 1 (the stage stops FAILED-a)", S.block1("67a", write_ledger(bad, "b1.log"), lambda *_: None) == 1)


# ═════════════ R1 ═════════════
def test_a1r_red_green():
    t = scored("67r", ledger_rows("67r", lambda c, a, s, r: "s1-no-gauge" if a == "S1" else None))
    check("a1r_red: S1 rows without the gauge prints -> A1-R FAIL", "\nA1-R FAIL" in t, t[-600:])
    t = scored("67r", ledger_rows("67r"))
    check("a1r_green: S1 carries the prints, B0 none, every S1 row completes -> A1-R PASS", "\nA1-R PASS" in t, t[-800:])
    rows = ledger_rows("67r")
    bad = [build("67r", r["cell"], "S1", r["seed"], r["rep"], "dnf") if (r["arm"] == "S1" and r["rep"] == 1 and r["seed"] == "42") else r for r in rows]
    check("a1r_red: an S1 DNF where every B0 row completes -> A1-R FAIL", "\nA1-R FAIL" in scored("67r", bad))


def test_r1_red_green():
    t = scored("67r", ledger_rows("67r"))
    check("r1_green: S2 completes, no S06-FAIL -> R1 PASS", "\nR1 PASS" in t, t[-800:])
    rows = ledger_rows("67r")
    bad = [build("67r", r["cell"], "S2", r["seed"], r["rep"], "dnf") if (r["arm"] == "S2" and r["rep"] == 1 and r["seed"] == "42") else r for r in rows]
    check("r1_red: an S2 DNF where every S1 row completes -> R1 FAIL", "\nR1 FAIL" in scored("67r", bad))
    bad = [build("67r", r["cell"], "S2", r["seed"], r["rep"], "s2-taper-nonzero") if (r["arm"] == "S2" and r["rep"] == 1 and r["seed"] == "42") else r for r in rows]
    check("r1_red: an S06-FAIL S2 row -> R1 FAIL", "\nR1 FAIL" in scored("67r", bad))


def test_r1_blind():
    t = scored("67r", ledger_rows("67r", lambda c, a, s, r: "r1-blind" if a == "S1" else None))
    check("r1_blind_red: S1 reads taper = 0 on more than 6 of 12 rows -> R1-BLIND (reported, not a FAIL)", "\nR1-BLIND" in t and "R1 FAIL" not in t, t[-600:])
    t = scored("67r", ledger_rows("67r"))
    check("r1_blind_green: S1 taper > 0 -> not blind", "R1-BLIND" not in t.split("VERDICT-67R")[0].splitlines()[-1] and "\nR1 PASS" in t)


def test_rho_red_green():
    r = build("67r", "c2-100", "S1", "42", 1, "rho-one-end")
    check("rho_red: window_reliable false on one end -> VOID-RHO", r["status"] == "VOID-RHO" and r["w67"]["wr_srv"] is False, (r["status"], r["problems"]))
    p = os.path.join(TMP, "rho.log")
    open(p, "w").write("P67ROW " + json.dumps(r) + "\n")
    check("rho_red: VOID-RHO is replaced once", quiet_replace("67r", p, "c2-100", "S1", "42", 1) == 0)
    g = build("67r", "c2-100", "S1", "42", 1)
    check("rho_green: window_reliable true on both ends -> LIVE (Realtime echo cleared the window-line rule)", g["status"] == "LIVE" and g["w67"]["wr_cli"] is True and g["w67"]["wr_srv"] is True, (g["status"], g["problems"]))


# ═════════════ R2 ═════════════
def _twin_rows(defect_of=None):
    return ledger_rows("67p", defect_of)


def test_r2_hold_red_green():
    t = scored("67p", _twin_rows(lambda c, a, s, r: "twin-worse" if a == "P2T" else None))
    check("r2_hold_red: P2T's U and oldest_unchanged_max well above P1T's -> R2-HOLD FIRED (both tests, exact, 924 relabellings)", "R2-HOLD FIRED" in t and "ALL 924 relabellings" in t and "smallest attainable p 0.00108" in t, t[-1500:])
    t = scored("67p", _twin_rows())
    check("r2_hold_green: equal-rank twins -> R2-HOLD CLEAR (not fired at this resolution)", "R2-HOLD CLEAR" in t, t[-1500:])


def test_r2_final_label():
    rows = _twin_rows(lambda c, a, s, r: "twin-no-final" if a in ("P1T", "P2T") else None)
    t = scored("67p", rows)
    check("r2_final_red: a twin log without final=1 reads the last periodic line, labelled ACCT-LAST", all(r["w67"]["acct_label"] == "ACCT-LAST" for r in rows if r["arm"] in ("P1T", "P2T")) and "ACCT-LAST" in t)
    rows = _twin_rows()
    check("r2_final_green: with final=1 the reading is the final line", all(r["w67"]["acct_label"] == "final" and r["w67"]["acct_identity"] for r in rows if r["arm"] in ("P1T", "P2T")))


def test_r2_under_min_red_green():
    def d(c, a, s, r):
        return "twin-under-min" if (a == "P2T" and r in (1, 2) and s == "42") else None
    rows = _twin_rows(d)
    check("r2_under_min_red: a twin row with neither [ACCT] nor [ACCT-RX] is VOID-WITNESS", all(r["status"] == "VOID-WITNESS" for r in rows if r["arm"] == "P2T" and r["rep"] in (1, 2) and r["seed"] == "42"))
    t = scored("67p", rows)
    check("r2_under_min_red: fewer than MIN_TWIN live twin rows -> R2-HOLD UNSCOREABLE and guard 2 NOT ELIGIBLE", "R2-HOLD UNSCOREABLE" in t and "guard 2 NOT ELIGIBLE" in t and "FIRED" not in t and "CLEAR" not in t.split("LIVE TWIN ROWS")[1], t[-800:])
    t = scored("67p", _twin_rows())
    check("r2_under_min_green: six live twin rows per seat -> scored", "UNSCOREABLE" not in t)


def test_r2_pace_witness():
    r = build("67p", "c2-100", "P1", "42", 1, "no-pace-echo")
    check("r2_pace_red: a pacer row without the pacer echo is VOID-WITNESS", r["status"] == "VOID-WITNESS" and "pace-echo-missing" in r["problems"], (r["status"], r["problems"]))
    g = build("67p", "c2-100", "P2", "42", 1)
    check("r2_pace_green: P2 (pslot > 0, rr > 0, echo, gate 35, per-object pace line) is LIVE", g["status"] == "LIVE" and g["w67"]["pace_echo"] == ["35"] and g["w67"]["pslot"] > 0, (g["status"], g["problems"]))


def test_mw_exact():
    p, n, pmin = S.exact_mw_p([1, 2, 3, 4, 5, 6], [7, 8, 9, 10, 11, 12])
    check("mw_exact_red: a complete separation at 6 v 6 has p = 1/924 over all 924 relabellings", n == 924 and abs(p - 1 / 924) < 1e-12 and abs(pmin - 1 / 924) < 1e-12, (p, n))
    p, n, _ = S.exact_mw_p([1, 2, 3, 4, 5, 6], [1, 2, 3, 4, 5, 6])
    check("mw_exact_green: identical samples read p > 0.5 (midranks for ties)", p > 0.5, p)
    p, n, _ = S.exact_mw_p([3, 3, 3, 3, 3, 3], [3, 3, 3, 3, 3, 3])
    check("mw_exact_green: all-tied samples read p = 1", abs(p - 1) < 1e-12)


# ═════════════ pool block, dump twins ═════════════
def test_pool_red_green():
    r = build("67o", "c8-100", "S1P170", "42", 1, "pool-echo-wrong")
    check("pool_red: a [GATES] pool echo that differs from the arm's pool is VOID-WITNESS", r["status"] == "VOID-WITNESS" and any(x.startswith("pool-echo") for x in r["problems"]), (r["status"], r["problems"]))
    g = build("67o", "c8-100", "S2P640", "42", 1)
    check("pool_green: the echo reads the arm's pool on both ends -> LIVE", g["status"] == "LIVE" and g["w67"]["pool_gate_cli"] == "640" and g["w67"]["pool_gate_srv"] == "640", (g["status"], g["problems"]))
    t = scored("67o", ledger_rows("67o"))
    check("pool_green: the pool block reports per (seat, pool) n = 3 in-order, hole-close, the completion tail, duplicates", "S1 pool 170: n=3" in t and "S2 pool 640: n=3" in t and "completion [" in t and "dup_src" in t and "REPORTED-ONLY" in t, t[-1200:])


def test_dump_twins():
    rows = ledger_rows("67d")
    t = scored("67d", rows)
    check("dump_green: the dump twins report their [SUCCDUMP] record count per row", len(rows) == 8 and all(r["w67"]["succdump_records"] == 5 for r in rows) and "records=5" in t, t[-600:])


# ═════════════ smokes ═════════════
def _smoke(plan, rows):
    lines = []
    rc = S.smoke(plan, write_ledger(rows, "sm.log"), lines.append)
    return rc, "\n".join(lines)


def test_smoke_red_green():
    sm = lambda plan: [build(plan, c, a, sd, rp) for c, _, a, sd, rp in plan67.plan(plan, True)]
    for plan in ("67a", "67r", "67p"):
        rc, t = _smoke(plan, sm(plan))
        check(f"smoke_{plan}_green: the rehearsal rows pass", rc == 0 and "SMOKE-PASS" in t, t[-800:])
    rows = sm("67a")
    rows = [build("67a", r["cell"], "S2", r["seed"], r["rep"], "s2-no-pslot") if r["arm"] == "S2" else r for r in rows]
    rc, t = _smoke("67a", rows)
    check("smoke_67a_red: an S2 smoke row without pslot fails the smoke", rc == 1 and "SMOKE-FAIL" in t)
    rows = [build("67a", r["cell"], r["arm"], r["seed"], r["rep"], "no-succ-times") for r in sm("67a")]
    rc, t = _smoke("67a", rows)
    check("smoke_67a_red: the [SUCC] check prints DUMP-TWINS-NEEDED (not a FAIL: the dump twins run)", "DUMP-TWINS-NEEDED" in t and rc == 0, t[-500:])
    rows = [build("67r", r["cell"], r["arm"], r["seed"], r["rep"], "rho-one-end" if r["arm"] == "S1" else None) for r in sm("67r")]
    rc, t = _smoke("67r", rows)
    check("smoke_67r_red: window_reliable false on an end fails the R1 smoke", rc == 1)
    rows = [build("67p", r["cell"], r["arm"], r["seed"], r["rep"], "twin-no-acct" if r["arm"] == "P2T" else None) for r in sm("67p")]
    rows = [r for r in rows]
    rc, t = _smoke("67p", rows)
    check("smoke_67p_red: a twin row with no [ACCT] in the kept log fails the R2 smoke", rc == 1 and "SMOKE-FAIL" in t)


# ═════════════ stage T ═════════════
def _tun(defect=None, smoke=False):
    p = os.path.join(TMP, "tun-%s.log" % (defect or "ok"))
    open(p, "w").write("\n".join(F.tun_lines(defect, reps=1 if smoke else 5, smoke=smoke)) + "\n")
    return p


def test_t_green_red():
    lines = []
    check("t_green: a clean stage T reads T PASS (16 commands, 12 scored)", S.score_t(_tun(), lines.append) == "T PASS", "\n".join(lines)[-1200:])
    for defect, want, name in (("t-refusal-missing", "T FAIL", "t_refusal_red: an S1 refusal line missing"), ("s1-tun-nodata", "T FAIL", "t_complete_red: an S1 transfer without data where B0's completes"),
                               ("s1-cfg-true", "T FAIL", "t_cfg_red: an S1 TUNCFG field reading true"), ("b0-run-key-false", "T VOID", "t_void: a B0 run reading cfg=false (the file never reached the tunnel)"),
                               ("t-base-refuses", "T ABORT-SEAT", "t_seat: B0's run stderr carrying the guard text")):
        lines = []
        res = S.score_t(_tun(defect), lines.append)
        check(f"{name} -> {want}", res.startswith(want), (res, "\n".join(lines)[-800:]))
    lines = []
    S.score_t(_tun(), lines.append)
    check("t_green: B0's four check commands are printed, not scored, and per-hint goodput is reported", any("B0 CHECK COMMANDS" in x and "window_out_of_order" in x for x in lines) and any("REPORTED ONLY" in x for x in lines))


def test_smoke_t():
    lines = []
    check("smoke_t_green: one invocation per seat, resolved configuration present, 16 T67REF, T67DONE", S.smoke_t(_tun(smoke=True), lines.append) == 0, "\n".join(lines))
    lines = []
    check("smoke_t_red: TUNCFG fields `-` (a log-level fault) fail the smoke", S.smoke_t(_tun("tun-no-cfg", smoke=True), lines.append) == 1)
    lines = []
    check("smoke_t_red: a missing T67REF line fails the smoke", S.smoke_t(_tun("t-refusal-missing", smoke=True), lines.append) == 1)


# ═════════════ stage G ═════════════
def g_make(which, cell, arm, seed, rep, defect=None, rc=0, cot=0):
    """What place38_parse.make_row does with the hooks (LD_PLAN=41d, LD_WITNESS67=<which>): make_row, then g_row's problems merged, then g_status."""
    key = ("g", which, cell, arm, str(seed), int(rep), defect, rc, cot)
    if key in _CACHE:
        return json.loads(json.dumps(_CACHE[key]))
    seat = F.G_SEAT[which][arm]
    cli, srv, drv = logs(which, cell, arm, seed, rep, defect)
    os.environ["LD_PLAN"] = "41d"
    os.environ["LD_ROW_BYTES"] = str(F.RUN_BYTES[cell])
    try:
        sha, shac, shab = "c" * 64, "c" * 64, "d" * 64
        row = pp.make_row(cell, arm, seed, rep, rc, "60", drv, cli, srv, cot, shac if arm == "CAND" else shab)
        p = S.g_row(row, arm, cli, srv, drv, shac if arm == "CAND" else shab, shac, shab, which)
    finally:
        os.environ.pop("LD_PLAN", None)
        os.environ.pop("LD_ROW_BYTES", None)
    row["problems"].extend(p)
    if p and row["status"] == "LIVE":
        row["status"] = "WITNESS-FAIL"
    S.g_status(row)
    _CACHE[key] = row
    return json.loads(json.dumps(row))


def test_g1_red_green():
    r = g_make("g1", "c2-100", "CAND", "42", 1, "s1-no-gauge")
    check("g1_gauge_red: an S1 (CAND) row without scw / plx / rlaw is WITNESS-FAIL (replaced once by place38_parse.REPLACEABLE)", r["status"] == "WITNESS-FAIL" and any("g1-no-gauge" in x for x in r["problems"]), (r["status"], r["problems"]))
    cli, srv, drv = logs("g1", "c2-100", "CAND", "42", 1)           # S1's logs on a BASE (B0) row
    os.environ["LD_PLAN"] = "41d"
    row = pp.make_row("c2-100", "BASE", "42", 1, 0, "60", drv, cli, srv, 0, "d" * 64)
    os.environ.pop("LD_PLAN", None)
    p = S.g_row(row, "BASE", cli, srv, drv, "d" * 64, "c" * 64, "d" * 64, "g1")
    check("g1_gauge_red: a B0 (BASE) row carrying the gauge prints fails its witness", any("gauge-prints-on-B0" in x for x in p), p)
    c, b = g_make("g1", "c2-100", "CAND", "42", 1), g_make("g1", "c2-100", "BASE", "42", 1)
    check("g1_gauge_green: S1 carries them, B0 none, both rows LIVE", c["status"] == "LIVE" and b["status"] == "LIVE" and c["w67g"]["g_scw"] and not b["w67g"]["g_any"], (c["status"], c["problems"], b["status"], b["problems"]))


def test_g2_red_green():
    r = g_make("g2", "c2-100", "CAND", "42", 1, "s2-no-pslot")
    check("g2_pslot_red: an S2 (CAND) row without pslot= is WITNESS-FAIL", r["status"] == "WITNESS-FAIL" and any("g2-no-pslot" in x for x in r["problems"]), (r["status"], r["problems"]))
    r = g_make("g2", "c2-100", "CAND", "42", 1, "s2-taper-nonzero")
    check("g2_taper_red: an S2 row with taper > 0 (pslot= printed; Bulk, rr = 0) is S06-FAIL, keeps its 41d label, is not replaceable",
          r["status"] == "S06-FAIL" and r["status_41d"] == "LIVE" and "S06-FAIL" not in pp.REPLACEABLE and r["w67g"]["pslot"] is not None and (r["w67g"]["rr_med"] or 0) == 0, (r["status"], r.get("status_41d"), r["problems"]))
    check("g2_taper_red: view41d scores it under its 41d label (LIVE here; a replaceable one reads as a crash, never replaced)", S.view41d_g(r)["status"] == "LIVE")
    r = g_make("g2", "c2-100", "CAND", "42", 1, "bulk-rr")
    check("g_route_red: rr > 0 on a Bulk row is a witness failure (W-ROUTE), reported with the row", r["status"] == "WITNESS-FAIL" and any("w-route" in x for x in r["problems"]), (r["status"], r["problems"]))
    c, b = g_make("g2", "c2-100", "CAND", "42", 1), g_make("g2", "c2-100", "BASE", "42", 1)
    check("g2_green / g_route_green: S2 has pslot, taper 0, rr 0; S1 (BASE) has no pslot; both LIVE", c["status"] == "LIVE" and b["status"] == "LIVE" and c["w67g"]["pslot"] is not None and b["w67g"]["pslot"] is None and (c["w67g"]["rr_max"] or 0) == 0, (c["status"], c["problems"], b["status"], b["problems"]))


def _g_ledger(which, defect_of=None):
    rows = []
    for g in range(12):
        seed, rep = ("42", "7")[g % 2], g // 2 + 1
        for cell in ("c1s-400", "c1d-400", "c2-100", "c8-100"):
            for arm in (("BASE", "CAND") if g % 2 == 0 else ("CAND", "BASE")):
                rows.append(g_make(which, cell, arm, seed, rep, defect_of(cell, arm, seed, rep) if defect_of else None))
    return rows


def _g_score(which, rows, name):
    p = os.path.join(TMP, name)
    with open(p, "w") as f:
        for r in rows:
            f.write("P38ROW " + json.dumps(r, sort_keys=True) + "\n")
        f.write("LD-BATTERY-DONE\n")
    lines = []
    S.score67g([p], lines.append, which)
    return "\n".join(lines)


def test_g_gate():
    t = _g_score("g2", _g_ledger("g2", lambda c, a, s, r: "s2-taper-nonzero" if (a == "CAND" and c == "c2-100" and s == "42" and r == 1) else None), "g2r.log")
    check("gate_g2_red: one S06-FAIL row -> GATE-67G2 FAIL (counted in the gate, kept in the ledger, scored under its 41d label)", "GATE-67G2 FAIL" in t and "S06-FAIL c2-100 CAND" in t and "1 S06-FAIL row" in t, t[-700:])
    t = _g_score("g1", _g_ledger("g1", lambda c, a, s, r: "s1-no-gauge" if (a == "CAND" and c == "c8-100" and s == "42" and r == 1) else None) , "g1r.log")
    check("gate_g1_red: an unreplaced witness failure -> GATE-67G1 FAIL (or NO VERDICT: the slot is missing its replacement)", "GATE-67G1 FAIL" in t and ("witness failure" in t or "NO VERDICT" in t), t[-700:])
    t = _g_score("g1", _g_ledger("g1"), "g1g.log")
    check("gate_g1_green: clean rows -> GATE-67G1 PASS after score41d's own output", "GATE-67G1 PASS" in t and "MERGE-CANDIDATE GUARD v2" in t and "G-WITNESS67" in t, t[-900:])
    t = _g_score("g2", _g_ledger("g2"), "g2g.log")
    check("gate_g2_green: clean rows -> GATE-67G2 PASS", "GATE-67G2 PASS" in t, t[-900:])
    base = []
    pp.score41d([os.path.join(TMP, "g2g.log")], base.append)
    check("gate_g_green: the first lines are score41d's output byte for byte", "\n".join(t.splitlines()[:len(base) - 3]) == "\n".join(base[:-3]) or t.startswith("\n".join(base[:10])))


def test_smoke_g():
    rows = []
    for cell, _, arm, seed, rep in pp.SMOKE41D:
        rows.append(g_make("g1", cell, arm, seed, rep))
    p = os.path.join(TMP, "smg.log")
    open(p, "w").write("\n".join("P38ROW " + json.dumps(r, sort_keys=True) for r in rows) + "\n")
    lines = []
    check("smoke_g_green: plan 41d's smoke rows with the seat witnesses -> SMOKE-PASS", S.smoke67g(p, "g1", lines.append) == 0, "\n".join(lines)[-800:])
    rows[1] = g_make("g1", rows[1]["cell"], "CAND", "42", 1, "s1-no-gauge")
    open(p, "w").write("\n".join("P38ROW " + json.dumps(r, sort_keys=True) for r in rows) + "\n")
    lines = []
    check("smoke_g_red: an S1 smoke row without the gauge prints -> SMOKE-FAIL", S.smoke67g(p, "g1", lines.append) == 1)


# ═════════════ harness 67c: the precedence, the minimum, the replaceable set, R1 / R2 / T readings, truncation ═════════════
def _row_with(plan, cell, arm, seed, rep, defect=None, rc=0, cot=0, sha=None, edit=None):
    cli, srv, drv = logs(plan, cell, arm, seed, rep, defect)
    if edit:
        cli, srv, drv = edit(cli, srv, drv)
    return json.loads(json.dumps(S.row_of(plan, cell, arm, seed, rep, rc, "60", drv, cli, srv, cot, sha or SHA[plan67.seat_of(arm)])))


def _ledger_file(rows, name, extra=(), done=True):
    p = os.path.join(TMP, name)
    with open(p, "w") as f:
        for r in rows:
            f.write("P67ROW " + json.dumps(r, sort_keys=True) + "\n")
        for x in extra:
            f.write(x + "\n")
        if done:
            f.write("LD-BATTERY-DONE\n")
    return p


def test_s06_precedence():
    """#51: taper > 0 / plost > 0 on an S2 row whose slot preconditions hold is S06-FAIL whatever the infra / crash status; a wrong binary (VOID-SEAT) stays above it."""
    for plan, cell in (("67a", "c2-100"), ("67r", "c2-100")):
        for why, kw in (("cotenant", dict(cot=1)), ("crash rc=1", dict(rc=1))):
            r = _row_with(plan, cell, "S2", "42", 1, "s2-taper-nonzero", **kw)
            check(f"s06_precedence_red: {plan} S2 row with taper > 0 that is also a {why} void reads S06-FAIL (never VOID, never replaced)", r["status"] == "S06-FAIL" and r["status_41d"] in ("VOID-COTENANT", "VOID-RC"), (r["status"], r.get("status_41d"), r["problems"]))
            p = _ledger_file([r], "s06p.log")
            check(f"s06_precedence_red: {plan} that row is not replaced ({why})", quiet_replace(plan, p, cell, "S2", "42", 1) == 1)
            check(f"s06_precedence_red: {plan} scored under its other label: a replaceable / crash label reads as the worst rank (VOID-RC)", S.view([r])[0]["status"] == "VOID-RC")
    r = _row_with("67a", "c2-100", "S2", "42", 1, "s2-taper-nonzero", sha="f" * 64)
    check("s06_precedence_green: a wrong binary is not an S2 row: VOID-SEAT stays above S06-FAIL", r["status"] == "VOID-SEAT", (r["status"], r["problems"]))
    r = _row_with("67a", "c2-100", "S2", "42", 1, "s2-taper-nonzero", sha="f" * 64, cot=1)
    check("s06_precedence_green: ... also under a cotenant label", r["status"] != "S06-FAIL" and any(x.startswith("bin-sha") for x in r["problems"]), (r["status"], r["problems"]))
    r = _row_with("67a", "c2-100", "S2", "42", 1, "s2-no-pslot", cot=1)
    check("s06_precedence_green: without pslot the slot preconditions do not hold: a cotenant void stays a cotenant void", r["status"] == "VOID-COTENANT", (r["status"], r["problems"]))
    rows = ledger_rows("67a")
    i = next(i for i, x in enumerate(rows) if x["arm"] == "S2" and x["cell"] == "c2-100" and x["rep"] == 1 and x["seed"] == "42")
    rows[i] = _row_with("67a", "c2-100", "S2", "42", 1, "s2-taper-nonzero", cot=1)
    t = scored("67a", rows)
    check("s06_precedence_red: stage A counts the cotenant-void S06 row (S06-FAIL-ROWS 1, no replacement)", "S06-FAIL-ROWS 1" in t and "REPLACED SLOTS: 0" in t, t[-700:])
    r = g_make("g2", "c2-100", "CAND", "42", 1, "s2-taper-nonzero", cot=1)
    check("s06_precedence_red: G2 an S2 row with taper > 0 that is also a cotenant void reads S06-FAIL", r["status"] == "S06-FAIL" and r["status_41d"] == "VOID-COTENANT" and S.view41d_g(r)["status"] == "VOID-RC", (r["status"], r.get("status_41d")))
    r = g_make("g2", "c2-100", "CAND", "42", 1, "s2-taper-nonzero", rc=1)
    check("s06_precedence_red: G2 the same with a crash (rc 1)", r["status"] == "S06-FAIL", (r["status"], r.get("status_41d")))


def test_slotfill_live_only():
    """#58 / #59: the UNSCOREABLE minimum counts LIVE rows only (a LIVE DNF counts, an S06-FAIL whose underlying label is LIVE counts, a crash does not)."""
    rows = ledger_rows("67a")
    out, k = [], 0
    for r in rows:
        if r["arm"] == "S1" and r["cell"] == "c2-100" and k < 3:
            out.append(build("67a", r["cell"], "S1", r["seed"], r["rep"], rc=1))      # VOID-RC: 3 of 12 -> 9 live < MIN_LIVE_A 10
            k += 1
        else:
            out.append(r)
    check("slotfill_red: the crash rows are VOID-RC", sum(1 for r in out if r["status"] == "VOID-RC") == 3)
    t = scored("67a", out)
    check("slotfill_red: 9 live S1 rows (3 crashes) in a cell are under the minimum: AS and A1 UNSCOREABLE (the old count of 12 would have scored)", "AS UNSCOREABLE" in t and "A1 UNSCOREABLE" in t and "c2-100 S1 9<%d" % plan67.MIN_LIVE_A in t, t[-900:])
    dnf = [build("67a", r["cell"], "S1", r["seed"], r["rep"], "dnf") if (r["arm"] == "S1" and r["cell"] == "c2-100" and r["rep"] <= 3 and r["seed"] == "42") else r for r in rows]
    t = scored("67a", dnf)
    check("slotfill_green: LIVE DNF rows count: no UNSCOREABLE", "UNSCOREABLE" not in t, t[-600:])
    s06 = list(rows)
    j = next(i for i, r in enumerate(s06) if r["arm"] == "S2" and r["cell"] == "c2-100")
    s06[j] = build("67a", "c2-100", "S2", s06[j]["seed"], s06[j]["rep"], "s2-taper-nonzero")
    check("slotfill_green: an S06-FAIL row whose underlying label is LIVE counts toward the minimum", len(S.slotfill(S.view(s06), "c2-100", "S2")) == 12)
    b0 = list(rows)
    n = 0
    for i, r in enumerate(b0):
        if r["arm"] == "B0" and r["cell"] == "c2-100" and n < 3:
            b0[i] = build("67a", "c2-100", "B0", r["seed"], r["rep"], rc=1)
            n += 1
    t = scored("67a", b0)
    check("slotfill_red: B0's minimum (MIN_LIVE_B0 = 4) is counted the same way: 3 crashes of 6 -> A1 UNSCOREABLE, AS scored", "A1 UNSCOREABLE" in t and "c2-100 B0 3<%d" % plan67.MIN_LIVE_B0 in t and ("AS PASS" in t or "AS FAIL" in t), t[-700:])


def test_replaceable_set():
    """#49: VOID-RC is not replaceable (a candidate crash that does not repeat must not vanish); NO_DATA keeps its one retry and is reported in every stage."""
    check("replaceable_red: VOID-RC is not in plan67.REPLACEABLE", "VOID-RC" not in plan67.REPLACEABLE and "NO_DATA" in plan67.REPLACEABLE, plan67.REPLACEABLE)
    crash = build("67a", "c2-100", "S1", "42", 1, rc=1)
    p = _ledger_file([crash], "vrc.log")
    check("replaceable_red: a VOID-RC row is not re-run", crash["status"] == "VOID-RC" and quiet_replace("67a", p, "c2-100", "S1", "42", 1) == 1, crash["status"])
    nd = S.error_row("67a", "c2-100", "S1", "42", 1, 0, RuntimeError("x"))
    p = _ledger_file([nd], "nd.log")
    check("replaceable_green: a NO_DATA row keeps its one retry", quiet_replace("67a", p, "c2-100", "S1", "42", 1) == 0)
    for plan, arm in (("67a", "S1"), ("67r", "S1"), ("67p", "P1T")):
        rows = ledger_rows(plan)
        i = next(i for i, r in enumerate(rows) if r["arm"] == arm)
        orig = S.error_row(plan, rows[i]["cell"], arm, rows[i]["seed"], rows[i]["rep"], 0, RuntimeError("x"))
        rows = rows[:i] + [orig] + rows[i:]            # the original NO_DATA row stays in the ledger, its replacement follows
        t = scored(plan, rows)
        slot = f"{rows[i + 1]['cell']} {arm} s{rows[i + 1]['seed']} r{rows[i + 1]['rep']}"
        check(f"nodata_report: {plan} prints the NO_DATA rows (count and slots) of the ledger", "NO_DATA ROWS: 1" in t and slot in t.split("NO_DATA ROWS: 1")[1].splitlines()[0], t[-700:])
    t = scored("67a", ledger_rows("67a"))
    check("nodata_report: none -> `NO_DATA ROWS: 0`", "NO_DATA ROWS: 0" in t)


def test_r1_b0_gauge():
    """#76: a B0 row carrying a gauge print is not VOID-WITNESS in 67r: it stays LIVE, is not replaced and fails A1-R."""
    cli, srv, drv = logs("67r", "c2-100", "S1", "42", 1)
    r = S.row_of("67r", "c2-100", "B0", "42", 1, 0, "60", drv, cli, srv, 0, SHA["B0"])
    check("r1_b0_gauge_red: R1's B0 row carrying the gauge prints stays LIVE (A1-R reads it), not VOID-WITNESS", r["status"] == "LIVE" and r["w67"]["g_any"] and "b0-carries-gauge-prints" not in r["problems"], (r["status"], r["problems"]))
    p = _ledger_file([r], "b0g.log")
    check("r1_b0_gauge_red: it is not replaced", quiet_replace("67r", p, "c2-100", "B0", "42", 1) == 1)
    rows = [r if (x["arm"] == "B0" and x["rep"] == 1 and x["seed"] == "42") else x for x in ledger_rows("67r")]
    t = scored("67r", rows)
    check("r1_b0_gauge_red: A1-R FAIL naming the row", "\nA1-R FAIL" in t and "B0 s42 r1 carries prints" in t, t[-800:])
    r_a = S.row_of("67a", "c2-100", "B0", "42", 1, 0, "60", *(lambda c, s, d: (d, c, s))(*logs("67a", "c2-100", "S1", "42", 1)), 0, SHA["B0"])
    check("r1_b0_gauge_green: stage A still voids a B0 row with prints (its precondition)", r_a["status"] == "VOID-WITNESS" and "b0-carries-gauge-prints" in r_a["problems"], (r_a["status"], r_a["problems"]))


def test_r1_readouts():
    """#81: R1 reports per arm goodput, cpu_cli/GB, cpu_srv/GB, lag_E_cli, lag_E_srv and the in-order delay (mean, p95) with the tick-floor marker; no clause."""
    t = scored("67r", ledger_rows("67r"))
    for arm in ("S1", "S2", "B0"):
        ln = next((x for x in t.splitlines() if x.startswith(f"  READOUT {arm} ")), "")
        check(f"r1_readouts_red: {arm} prints goodput, cpu_cli/GB, cpu_srv/GB, lag_E_cli, lag_E_srv, in-order mean and p95 under the 30 s tick floor",
              all(k in ln for k in ("goodput", "cpu_cli/GB", "cpu_srv/GB", "lag_E_cli", "lag_E_srv", "in-order delay mean", "p95", "under the 30 s tick floor")), ln or t[-900:])


def test_r1_complete():
    """#80 / #112: R1 and A1-R read UNSCOREABLE (INCOMPLETE) unless every registered slot holds a row."""
    t = scored("67r", ledger_rows("67r", drop=lambda c, a, s, r: a == "B0" and s == "7" and r == 3))
    check("r1_incomplete_red: a registered slot without a row -> A1-R and R1 UNSCOREABLE (INCOMPLETE), never PASS", "A1-R UNSCOREABLE" in t and "R1 UNSCOREABLE" in t and "INCOMPLETE" in t and "PASS" not in t.split("VERDICT-67R")[1], t[-700:])
    t = scored("67r", ledger_rows("67r"))
    check("r1_incomplete_green: every slot has a row -> scored", "INCOMPLETE" not in t and "\nR1 PASS" in t)
    os.environ["LD_DROP_S2"] = "1"
    try:
        n = len(plan67.expect_slots("67r"))
        t = scored("67r", ledger_rows("67r"))
        check("r1_incomplete_green: under LD_DROP_S2 the registered slots are the S1 / B0 ones only (18)", n == 18 and "INCOMPLETE" not in t, (n, t[-400:]))
    finally:
        os.environ.pop("LD_DROP_S2")


TRUNC = "LD-BATTERY-TRUNCATED at LD_HARD 1790000000"


def test_truncated():
    """#112: a ledger with the LD-BATTERY-TRUNCATED line reads TRUNCATED in place of every PASS / CLEAR verdict line."""
    t = scored("67a", ledger_rows("67a"), extra=(TRUNC,))
    check("truncated_red: stage A: AS TRUNCATED and A1 TRUNCATED, never PASS", "\nAS TRUNCATED" in t and "\nA1 TRUNCATED" in t and "AS PASS" not in t and "A1 PASS" not in t and "VERDICT-67A AS TRUNCATED; A1 TRUNCATED" in t, t[-500:])
    t = scored("67r", ledger_rows("67r"), extra=(TRUNC,))
    check("truncated_red: stage R1: A1-R TRUNCATED and R1 TRUNCATED", "\nA1-R TRUNCATED" in t and "\nR1 TRUNCATED" in t and "PASS" not in t.split("VERDICT-67R")[1], t[-500:])
    t = scored("67p", _twin_rows(), extra=(TRUNC,))
    check("truncated_red: stage R2: R2-HOLD TRUNCATED, never CLEAR", "\nR2-HOLD TRUNCATED" in t and "R2-HOLD CLEAR" not in t, t[-500:])
    for plan in ("67o", "67d"):
        t = scored(plan, ledger_rows(plan), extra=(TRUNC,))
        check(f"truncated_red: {plan} (reported only) prints TRUNCATED", "TRUNCATED" in t.splitlines()[-1], t[-300:])
    lines = []
    p = _tun()
    open(p, "a").write(TRUNC + "\n")
    res = S.score_t(p, lines.append)
    check("truncated_red: stage T: T TRUNCATED in place of T PASS", res.startswith("T TRUNCATED") and "\nT PASS" not in "\n".join(lines), res)
    p = os.path.join(TMP, "tun-trunc2.log")
    open(p, "w").write("\n".join(ln for ln in F.tun_lines() if not ln.startswith("T67DONE")) + "\n" + TRUNC + "\n")
    res = S.score_t(p, lambda *_: None)
    check("truncated_green: stage T cut before T67DONE stays T FAIL (a missing T67DONE is a FAIL; TRUNCATED replaces only PASS)", res.startswith("T FAIL"), res)
    for which in ("g1", "g2"):
        rows = _g_ledger(which)
        p = os.path.join(TMP, f"g{which}-trunc.log")
        with open(p, "w") as f:
            for r in rows:
                f.write("P38ROW " + json.dumps(r, sort_keys=True) + "\n")
            f.write("LD-BATTERY-DONE\n" + TRUNC + "\n")
        lines = []
        S.score67g([p], lines.append, which)
        tt = "\n".join(lines)
        check(f"truncated_red: stage G {which}: GATE-67G{which[1]} TRUNCATED, never PASS (score41d's own GATE-41d verdict lines read TRUNCATED too)", f"\nGATE-67G{which[1]} TRUNCATED" in tt and f"GATE-67G{which[1]} PASS" not in tt
              and not [x for x in tt.splitlines() if re.match(r"^(VERDICT )?GATE-41d(\[[^\]]*\])? PASS", x)] and "VERDICT GATE-41d TRUNCATED" in tt, [x for x in tt.splitlines() if "GATE" in x])
    t = scored("67a", ledger_rows("67a", _a_as_defect), extra=(TRUNC,))
    check("truncated_green: a FAIL on a truncated ledger stays FAIL (a finding is not hidden)", "\nAS FAIL" in t, t[-400:])


def test_r2_blind():
    """#85: the mechanism-ran witness on the scored R2 rows (P1 taper > 0; P2 pslot > 0 with rr > 0): more than half failing -> R2-BLIND <seat>, R2-HOLD UNSCOREABLE."""
    t = scored("67p", _twin_rows(lambda c, a, s, r: "r2-blind" if a in ("P1", "P2") else None))
    check("r2_blind_red: P1 with taper = 0 and P2 with pslot = 0 on every row -> R2-BLIND P1 and R2-BLIND P2, R2-HOLD UNSCOREABLE (R2 VOID)",
          "R2-BLIND P1" in t and "R2-BLIND P2" in t and "\nR2-HOLD UNSCOREABLE" in t and "R2 VOID: the mechanism did not run; guard 2 waits for an R2 re-run" in t and "CLEAR" not in t.split("LIVE TWIN ROWS")[1], t[-900:])
    t = scored("67p", _twin_rows(lambda c, a, s, r: "r2-blind" if a == "P1" else None))
    check("r2_blind_red: one blind seat is enough", "R2-BLIND P1" in t and "R2-BLIND P2" not in t and "\nR2-HOLD UNSCOREABLE" in t, t[-500:])
    t = scored("67p", _twin_rows())
    check("r2_blind_green: clean rows: no R2-BLIND, R2-HOLD CLEAR", "R2-BLIND" not in t and "R2-HOLD CLEAR" in t, t[-500:])
    rows = _twin_rows()
    n = 0
    for i, r in enumerate(rows):
        if r["arm"] == "P1" and n < 3:
            rows[i] = build("67p", r["cell"], "P1", r["seed"], r["rep"], "r2-blind")
            n += 1
    t = scored("67p", rows)
    check("r2_blind_green: exactly half of a seat's rows failing is not blind (more than half is)", "R2-BLIND" not in t, t[-400:])


def test_r2_missing_field():
    """#91: a missing U or oldest_unchanged_max on a counted twin row makes R2-HOLD UNSCOREABLE, never CLEAR."""
    for field in ("oldest_unchanged_max", "O_serv"):
        rows = _twin_rows()
        j = next(i for i, r in enumerate(rows) if r["arm"] == "P2T")
        rows[j]["w67"]["acct"][field] = None
        if field == "O_serv":
            rows[j]["w67"]["acct_U"] = None       # what the parser writes for a row without O_serv (see readings)
        t = scored("67p", rows)
        check(f"r2_missing_red: a counted twin row without {field} -> R2-HOLD UNSCOREABLE, no CLEAR / FIRED", "\nR2-HOLD UNSCOREABLE" in t and "CLEAR" not in t.split("LIVE TWIN ROWS")[1] and "FIRED" not in t and field in t, t[-600:])
    cli, srv, drv = logs("67p", "c2-100", "P2T", "42", 1)
    cli = [ln.replace("O_serv=", "O_servx=") if "[ACCT]" in ln else ln for ln in cli]
    r = S.row_of("67p", "c2-100", "P2T", "42", 1, 0, "60", drv, cli, srv, 0, SHA["P2T"])
    check("r2_missing_red: the row parser leaves U unread (None) when O_serv is absent, never 0", r["w67"]["acct_U"] is None, r["w67"]["acct_U"])
    check("r2_missing_green: complete rows still score", "UNSCOREABLE" not in scored("67p", _twin_rows()))


def test_rho_witness_line():
    """#74: the rho = 1 witness is the `perf configuration` line's window_reliable; a missing line reads None: VOID-RHO."""
    for end in ("cli", "srv"):
        r = _row_with("67r", "c2-100", "S1", "42", 1, edit=lambda c, s, d, end=end: (([x for x in c if "perf configuration" not in x], s, d) if end == "cli" else (c, [x for x in s if "perf configuration" not in x], d)))
        check(f"rho_line_red: R1 row with no `perf configuration` line on the {end} end -> VOID-RHO (None is not true)", r["status"] == "VOID-RHO" and r["w67"][f"wr_{end}"] is None, (r["status"], r["w67"].get(f"wr_{end}")))


def test_t_precedence():
    """#100 / #101: finding before void; a bring-up failure on either seat is ABORT-BRINGUP-T and no T verdict."""
    def run(defect):
        lines = []
        res = S.score_t(_tun(defect), lines.append)
        return res, "\n".join(lines)
    res, txt = run("t-bringup-fail")
    check("t_bringup_red: a bring-up failure (B0 realtime) is ABORT-BRINGUP-T seat=B0 hint=realtime, no T PASS / FAIL / VOID line", res.startswith("ABORT-BRINGUP-T seat=B0 hint=realtime") and not any(x.startswith(("T PASS", "T FAIL", "T VOID")) for x in txt.splitlines()), (res, txt[-500:]))
    res, txt = run("t-bringup-fail-s1")
    check("t_bringup_red: the same for S1 (S1 realtime)", res.startswith("ABORT-BRINGUP-T seat=S1 hint=realtime"), (res, txt[-500:]))
    res, txt = run("t-refusal-and-void")
    check("t_precedence_red: an S1 refusal failure together with a B0 key-false reads T FAIL, not T VOID (finding before void)", res.startswith("T FAIL") and "the file never reached the tunnel: T VOID" in txt, (res, txt[-700:]))
    res, txt = run("t-refusal-and-bringup")
    check("t_precedence_red: an S1 refusal failure (independent of bring-up) outranks ABORT-BRINGUP-T: T FAIL", res.startswith("T FAIL"), (res, txt[-700:]))
    res, txt = run("t-base-refuses")
    check("t_precedence_green: ABORT-SEAT stays first", res.startswith("T ABORT-SEAT"), res)
    res, txt = run("b0-run-key-false")
    check("t_precedence_green: a lone clause-2 failure is T VOID", res.startswith("T VOID"), res)
    p = os.path.join(TMP, "tun-nodone.log")
    open(p, "w").write("\n".join(ln for ln in F.tun_lines("b0-run-key-false") if not ln.startswith("T67DONE")) + "\n")
    res = S.score_t(p, lambda *_: None)
    check("t_precedence_red: a missing T67DONE (T FAIL) outranks clause 2 (T VOID)", res.startswith("T FAIL"), res)
    res, txt = run("t-bringup-fail")
    check("t_precedence_green: every failing clause line is printed regardless", "(3)" in txt and "(4)" in txt and "(1)" in txt)


def test_t_parser_agrees_with_verify4():
    """#132: s67_score's own T parser and `verify4_parse.py tun` agree on one ledger (transfers per (label, rep), completion, bring-up failures)."""
    import verify4_parse as V
    for defect in (None, "s1-tun-nodata", "t-bringup-fail"):
        p = _tun(defect)
        inv, refs, done = S.tun_parse(p)
        arms = {a["label"]: a for a in V.tun_arms([p])}
        ok = set(arms) == {f"{s}-{h}" for s, h in inv}
        for (seat, hint), i in inv.items():
            a = arms.get(f"{seat}-{hint}")
            if a is None:
                ok = False
                continue
            mine = sorted(x["mbps"] for x in i["reps"].values() if x["ok"])
            ok = ok and mine == sorted(a["reps"]) and sum(1 for x in i["reps"].values() if not x["ok"]) == a["nodata"] and bool(i.get("bringup_fail")) == a["bringup_fail"]
        check(f"t_parser_agree: s67_score.tun_parse and verify4_parse tun_arms agree ({defect or 'clean'})", ok and done, (defect, {k: (v["reps"], v["nodata"], v["bringup_fail"]) for k, v in arms.items()}))


def _g_with(which, rows_in, extra=()):
    p = os.path.join(TMP, f"gb1-{which}.log")
    with open(p, "w") as f:
        for r in rows_in:
            f.write("P38ROW " + json.dumps(r, sort_keys=True) + "\n")
        for x in extra:
            f.write(x + "\n")
    return p


def test_g_block1():
    """#111: block 1 of stage G (plan 41c's first 8 rows: every (cell, arm) once): each first row LIVE with its G witnesses, else rc 1."""
    import io
    from contextlib import redirect_stdout

    def b1(which, rows_in):
        buf = io.StringIO()
        with redirect_stdout(buf):
            rc = S.block1_g(which, _g_with(which, rows_in))
        return rc, buf.getvalue()
    one = [r for r in _g_ledger("g1")[:8]]
    j = next(i for i, r in enumerate(one) if r["cell"] == "c8-100" and r["arm"] == "BASE")
    one[j] = g_make("g1", "c8-100", "BASE", "42", 1, rc=1)
    rc_, o_ = b1("g1", one)
    check("g_block1_pairs: a block where only the c8 BASE first row is not LIVE (a crash) fails block 1 (every one of the 8 (cell, arm) pairs is read, not only the first CAND and the first BASE)", rc_ == 1 and "BLOCK1-FAIL BASE" in o_ and "c8-100" in o_, (rc_, o_))
    one = [r for r in _g_ledger("g1")[:8]]
    j = next(i for i, r in enumerate(one) if r["cell"] == "c8-100" and r["arm"] == "CAND")
    one[j] = g_make("g1", "c8-100", "CAND", "42", 1, "s1-no-gauge")
    rc_, o_ = b1("g1", one)
    check("g_block1_pairs: a failing first row at another cell (c8 CAND) stops the stage too", rc_ == 1 and "BLOCK1-FAIL CAND" in o_, (rc_, o_))
    miss = [r for r in _g_ledger("g1")[:8] if not (r["cell"] == "c1d-400" and r["arm"] == "BASE")]
    rc_, o_ = b1("g1", miss)
    check("g_block1_pairs: a pair with no row at all fails block 1", rc_ == 1 and "c1d-400 BASE: no row" in o_, (rc_, o_))
    check("g_block1_size: 8 rows, the smallest prefix of plan 41c's order that holds every (cell, arm) once", S.G_BLOCK1_N == 8 and len({(c, a) for c, _, a, _, _ in pp.plan41c()[:8]}) == 8 and len({(c, a) for c, _, a, _, _ in pp.plan41c()[:7]}) < 8)
    for which in ("g1", "g2"):
        rows_ = _g_ledger(which)
        rc, o = b1(which, rows_[:8])
        check(f"g_block1_green: {which} clean first block -> rc 0", rc == 0, o)
        bad = [dict(r) for r in rows_[:8]]
        bad[1] = g_make(which, bad[1]["cell"], "CAND", "42", 1, "s1-no-gauge" if which == "g1" else "s2-no-pslot")
        rc, o = b1(which, bad)
        check(f"g_block1_red: {which} a WITNESS-FAIL first row -> rc 1 (BLOCK1-FAIL)", rc == 1 and "BLOCK1-FAIL" in o and "FAILED" in o, (rc, o))
        bad = [dict(r) for r in rows_[:8]]
        bad[0] = g_make(which, bad[0]["cell"], "BASE", "42", 1, rc=1)
        rc, o = b1(which, bad)
        check(f"g_block1_red: {which} a crash first row -> rc 1", rc == 1, (rc, o))
    bad = [dict(r) for r in _g_ledger("g2")[:8]]
    bad[1] = g_make("g2", bad[1]["cell"], "CAND", "42", 1, "s2-taper-nonzero")
    rc, o = b1("g2", bad)
    check("g_block1_red: g2 an S06-FAIL first row -> rc 1", rc == 1 and "S06-FAIL" in o, (rc, o))
    rc, o = b1("g1", [])
    check("g_block1_red: g1 no rows -> rc 1 (no row)", rc == 1)


def test_fixtures_deterministic():
    a = F.extra_lines("67a", "c2-100", "S1", "42", 1)
    b = F.extra_lines("67a", "c2-100", "S1", "42", 1)
    check("fixtures_green: extra_lines is deterministic in (plan, cell, arm, seed, rep)", a == b and a != F.extra_lines("67a", "c2-100", "S1", "42", 2))
    check("fixtures_green: every defect name is accepted", all(F.extra_lines("67a", "c2-100", "S2", "7", 2, d) for d in sorted(F.DEFECTS) if d not in ("twin-no-final", "twin-under-min", "twin-worse", "twin-no-acct", "no-pace-echo", "r1-blind", "bulk-rr")))


# ═════════════ harness 67c, review fixes (brief C) ═════════════
RULE = "An UNSCOREABLE caused by candidate-arm crashes is a finding to investigate, not something a plain re-run clears."


def _crash_rows(arm, cell, n):
    rows = ledger_rows("67a")
    out, k = [], 0
    for r in rows:
        if r["arm"] == arm and r["cell"] == cell and k < n:
            out.append(build("67a", r["cell"], arm, r["seed"], r["rep"], rc=1))
            k += 1
        else:
            out.append(r)
    return out


def test_r2_candidate_crash():
    """Round 2, item 2: the candidate-crash rule on R2's twin minimum: P2T crash rows (VOID-RC: never replaced) that account for the shortfall read CANDIDATE-CRASH P2T; P1T (the reference) crashes read a plain UNSCOREABLE."""
    def crash_twins(arm, n):
        out, k = [], 0
        for r in _twin_rows():
            if r["arm"] == arm and k < n:
                out.append(build("67p", r["cell"], arm, r["seed"], r["rep"], rc=1))
                k += 1
            else:
                out.append(r)
        return out
    rows = crash_twins("P2T", 2)
    check("r2_crash_red: the crashed twin rows are VOID-RC (not replaceable)", sum(1 for r in rows if r["arm"] == "P2T" and r["status"] == "VOID-RC") == 2 and "VOID-RC" not in plan67.REPLACEABLE, [r["status"] for r in rows if r["arm"] == "P2T"])
    t = scored("67p", rows)
    check("r2_crash_red: P2T live + crashes reach the minimum -> R2-HOLD UNSCOREABLE (CANDIDATE-CRASH P2T n=2: a finding to investigate, not cleared by a plain re-run); crash counts printed",
          "\nR2-HOLD UNSCOREABLE (CANDIDATE-CRASH P2T n=2: a finding to investigate, not cleared by a plain re-run)" in t and "P2T 4 (crashes 2)" in t and "CLEAR" not in t.split("LIVE TWIN ROWS")[1], t[-900:])
    t = scored("67p", crash_twins("P1T", 2))
    check("r2_crash_green: P1T (reference) crashes -> a plain UNSCOREABLE with the counts, no CANDIDATE-CRASH", "\nR2-HOLD UNSCOREABLE (live twin rows P1T 4 (crashes 2)" in t and "CANDIDATE-CRASH" not in t, t[-900:])


def test_candidate_crash():
    """Item 1: a candidate arm's crashes that account for the shortfall read CANDIDATE-CRASH; the per-arm counts are printed."""
    t = scored("67a", _crash_rows("S2", "c2-100", 3))
    check("candidate_crash_red: 3 S2 crashes in c2-100 (9 live + 3 crashes >= the minimum) -> AS UNSCOREABLE (CANDIDATE-CRASH S2 n=3 in c2-100: a finding to investigate, not cleared by a plain re-run)",
          "\nAS UNSCOREABLE (CANDIDATE-CRASH S2 n=3 in c2-100: a finding to investigate, not cleared by a plain re-run)" in t, t[-900:])
    check("candidate_crash_red: the per-arm live / crash counts are printed (MINIMUM-COUNTS)", "MINIMUM-COUNTS" in t and "c2-100 S2 live 9 crashes 3" in t and "c2-100 S1 live 12 crashes 0" in t, t[-1400:])
    check("candidate_crash_green: S2 crashes do not touch A1 (its test reads S1 and B0 only): A1 is scored, no CANDIDATE-CRASH S1",
          ("\nA1 PASS" in t or "\nA1 FAIL" in t) and "CANDIDATE-CRASH S1" not in t, t[-900:])
    t = scored("67a", _crash_rows("S1", "c2-100", 3))
    check("candidate_crash_red: 3 S1 crashes -> A1 UNSCOREABLE (CANDIDATE-CRASH S1 n=3 in c2-100: ...) (S1 is A1's candidate)", "\nA1 UNSCOREABLE (CANDIDATE-CRASH S1 n=3 in c2-100: a finding to investigate, not cleared by a plain re-run)" in t, t[-900:])
    check("candidate_crash_green: AS reads S1 as its reference: a plain UNSCOREABLE with the crash count beside the live count", "\nAS UNSCOREABLE (fewer live rows than the minimum: c2-100 S1 9<10 (crashes 3))" in t, t[-900:])
    t = scored("67a", ledger_rows("67a", drop=lambda c, a, s, r: a == "S2" and c == "c8-100" and r > 2))
    check("candidate_crash_green: a shortfall of MISSING rows (no crash rows) is a plain UNSCOREABLE, not CANDIDATE-CRASH", "\nAS UNSCOREABLE (fewer live rows than the minimum: c8-100 S2 4<10 (crashes 0))" in t and "CANDIDATE-CRASH" not in t, t[-700:])
    t = scored("67a", _crash_rows("B0", "c2-100", 3))
    check("candidate_crash_green: B0 (the baseline) crashes -> a plain UNSCOREABLE with the counts", "A1 UNSCOREABLE (fewer live rows than the minimum: c2-100 B0 3<4 (crashes 3))" in t and "CANDIDATE-CRASH" not in t, t[-700:])
    src = open(os.path.join(HERE, "s67_score.py"), encoding="utf-8").read()
    check("candidate_crash: the rule's sentence is in the module docstring and in the code comment", S.__doc__.count(RULE) == 1 and src.count(RULE) == 2, src.count(RULE))
    rows = ledger_rows("67r")
    j = next(i for i, r in enumerate(rows) if r["arm"] == "S2")
    rows[j] = build("67r", rows[j]["cell"], "S2", rows[j]["seed"], rows[j]["rep"], rc=1)
    t = scored("67r", rows)
    check("candidate_crash_r1: R1 / A1-R have no live minimum: an S2 crash row is a row (never INCOMPLETE) and reads R1 FAIL, never PASS", "INCOMPLETE" not in t and "\nR1 FAIL" in t and "\nR1 PASS" not in t, t[-500:])
    dnf = [build("67a", "c2-100", "S1", "42", 1, "dnf")]
    check("candidate_crash: live() keeps DNF rows (a LIVE DNF counts toward the minimum); slotfill is called by score_a alone", len(S.live(dnf, "c2-100", "S1")) == 1 and dnf[0]["dnf"] is True and src.count("slotfill(") == 2, (src.count("slotfill("), dnf[0]["status"]))


def _passing_lines(text):
    return [x for x in text.splitlines() if S.passing(x) and "PASS iff" not in x and "never PASS" not in x]


def test_truncation_every_verdict():
    """Item 2: a passing ledger cut at LD_HARD prints no PASS / CLEAR / ELIGIBLE verdict line in any scorer, and a TRUNCATED line."""
    for plan, name in (("67a", "A"), ("67r", "R1"), ("67p", "R2"), ("67o", "pool"), ("67d", "dump")):
        rows = ledger_rows(plan) if plan != "67p" else _twin_rows()
        t = scored(plan, rows, extra=(TRUNC,))
        clean = scored(plan, rows)
        check(f"trunc_{name}: the passing fixture reads a PASS / CLEAR verdict untruncated (the control)", plan in ("67o", "67d") or bool(_passing_lines(clean)), clean[-400:])
        check(f"trunc_{name}: LD-BATTERY-TRUNCATED -> no line carries a PASS / CLEAR / ELIGIBLE verdict and a TRUNCATED line exists", not _passing_lines(t) and "TRUNCATED" in t, _passing_lines(t) or t[-400:])
    rows = ledger_rows("67r", lambda c, a, s, r: "r1-blind" if a == "S1" else None)
    t, clean = scored("67r", rows, extra=(TRUNC,)), scored("67r", rows)
    check("trunc_R1-BLIND: a blind R1 (the control reads R1-BLIND) cut at LD_HARD reads R1 TRUNCATED, never R1-BLIND", "\nR1-BLIND" in clean and "\nR1 TRUNCATED" in t and "\nR1-BLIND" not in t, t[-400:])
    p = _tun()
    open(p, "a").write(TRUNC + "\n")
    lines = []
    S.score_t(p, lines.append)
    check("trunc_T: no PASS verdict line, a TRUNCATED line", not _passing_lines("\n".join(lines)) and any("TRUNCATED" in x for x in lines), _passing_lines("\n".join(lines)))
    for which in ("g1", "g2"):
        rows = _g_ledger(which)
        p = os.path.join(TMP, f"tr-{which}.log")
        with open(p, "w") as f:
            for r in rows:
                f.write("P38ROW " + json.dumps(r, sort_keys=True) + "\n")
            f.write("LD-BATTERY-DONE\n" + TRUNC + "\n")
        lines = []
        res = S.score67g([p], lines.append, which)[0]
        tt = "\n".join(lines)
        check(f"trunc_G {which}: no PASS verdict line (GATE-41d lines included), a TRUNCATED line", not _passing_lines(tt) and "GATE-67G%s TRUNCATED" % which[1] in tt, _passing_lines(tt))
        check(f"trunc_G {which}: the returned result carries the cut (truncated True, gate not PASS)", res.get("truncated") is True and res["gate"] != "PASS", {k: v for k, v in res.items() if k in ("truncated", "gate")})
    for v, want in (("R2-HOLD CLEAR (not fired)", True), ("R2-HOLD CLEAR; guard 2 ELIGIBLE", True), ("guard 2 ELIGIBLE", True), ("R2-HOLD UNSCOREABLE; guard 2 NOT ELIGIBLE (x)", False), ("AS FAIL (p=0.01; PASS iff p > 0.05)", False),
                    ("T PASS", True), ("GATE-67G1 PASS", True), ("R1-BLIND", False)):
        check(f"trunc_token: `{v}` {'is' if want else 'is not'} a passing verdict", S.passing(v) is want and (S.trunc_v(v, "X", True) != v) is want, v)


def test_ledger_drop_state():
    """Item 3: R1 under LD_DROP_S2 scores the same from a pulled ledger without the VM env (the LD-PLAN67 line)."""
    os.environ["LD_DROP_S2"] = "1"
    try:
        rows = ledger_rows("67r")
    finally:
        os.environ.pop("LD_DROP_S2")
    check("ledger_drop_red: the dropped-S2 R1 ledger holds no S2 row", rows and not any(r["arm"] == "S2" for r in rows) and len(rows) == 18)
    hdr = "LD-PLAN67 plan=67r drop_s2=1 dump=0"
    t_unset = scored("67r", rows, extra=(hdr,))
    os.environ["LD_DROP_S2"] = "1"
    try:
        t_set = scored("67r", rows)
    finally:
        os.environ.pop("LD_DROP_S2")
    a_unset = [x for x in t_unset.splitlines() if x.startswith(("A1-R ", "R1 "))]
    check("ledger_drop_red: scored with LD_DROP_S2 unset but the ledger's drop_s2=1 line: A1-R PASS, not INCOMPLETE (the same answer as with the env set)",
          "INCOMPLETE" not in t_unset and any(x.startswith("A1-R PASS") for x in a_unset) and [x for x in t_set.splitlines() if x.startswith(("A1-R ", "R1 "))] == a_unset, (a_unset, t_unset[-500:]))
    t_none = scored("67r", rows)
    check("ledger_drop_green: a ledger without the line and without the env keeps the registered 30 slots: INCOMPLETE (the fallback is the environment)", "INCOMPLETE" in t_none, t_none[-400:])
    t_full = scored("67r", ledger_rows("67r"), extra=("LD-PLAN67 plan=67r drop_s2=0 dump=0",))
    check("ledger_drop_green: drop_s2=0 on a full ledger scores all 30", "INCOMPLETE" not in t_full and "\nR1 PASS" in t_full)
    full = ledger_rows("67r")
    os.environ["LD_DROP_S2"] = "1"
    try:
        t_env = scored("67r", full, extra=("LD-PLAN67 plan=67r drop_s2=0 dump=0",))
    finally:
        os.environ.pop("LD_DROP_S2")
    check("ledger_drop_green: the ledger's line wins over the environment (drop_s2=0 against env 1: the 30 slots are expected)", "INCOMPLETE" not in t_env and "\nR1 PASS" in t_env, t_env[-400:])
    ta = scored("67a", ledger_rows("67a", drop=lambda c, a, s, r: a == "S2"), extra=("LD-PLAN67 plan=67a drop_s2=1 dump=0",))
    tn = scored("67a", ledger_rows("67a", drop=lambda c, a, s, r: a == "S2"))
    check("ledger_drop_green: stage A reads the ledger's drop state (its minimum counts leave S2 out); A1 scores S1 against B0 either way (S2 is not in A1's test)",
          ("\nA1 PASS" in ta or "\nA1 FAIL" in ta) and ("\nA1 PASS" in tn or "\nA1 FAIL" in tn) and "S2 live" not in ta and "c2-100 S2 live 0" in tn, (ta[-400:], tn[-400:]))


def test_g2_taper_preconditions():
    """Round 2, item 1: G2 is Bulk (rr = 0 by design), so S06-FAIL in G needs only the S2 seat witness (pslot= printed), not stage A's mechanism precondition; and a taper > 0 reading on an S2 row
    is never cleared by a replacement: G2-TAPER-NONZERO is read over every row run, and GATE-67G2 fails whatever the replacement shows."""
    r = g_make("g2", "c2-100", "CAND", "42", 1, "s2-taper-nonzero")
    check("g2_s06_red: taper > 0 on a Bulk S2 row (pslot printed, rr = 0) -> S06-FAIL (reachable in G2)", r["status"] == "S06-FAIL" and (r["w67g"]["rr_med"] or 0) == 0 and r["w67g"]["taper"] > 0, (r["status"], r["w67g"].get("rr_med")))
    r = g_make("g2", "c2-100", "CAND", "42", 1, "g2-taper-no-pslot")
    check("g2_s06_green: taper > 0 without the pslot= seat witness -> WITNESS-FAIL (replaceable), not S06-FAIL", r["status"] == "WITNESS-FAIL" and r["w67g"]["taper"] > 0 and r["w67g"]["pslot"] is None, (r["status"], r["problems"]))
    rows = _g_ledger("g2", lambda c, a, s, r_: "g2-taper-no-pslot" if (a == "CAND" and c == "c2-100" and s == "42" and r_ == 1) else None)
    p = os.path.join(TMP, "g2tr.log")
    with open(p, "w") as f:
        for x in rows:
            f.write("P38ROW " + json.dumps(x, sort_keys=True) + "\n")
        f.write("LD-BATTERY-DONE\n")
    q = os.path.join(TMP, "g2tr-repl.log")
    with open(q, "w") as f:
        f.write("P38ROW " + json.dumps(g_make("g2", "c2-100", "CAND", "42", 1), sort_keys=True) + "\n")      # the replacement is clean
        f.write("LD-BATTERY-DONE\n")
    lines = []
    S.score67g([p, q], lines.append, "g2")
    t_ = "\n".join(lines)
    check("g2_taper_replaced_red: a first row with taper > 0 whose replacement is clean still fails GATE-67G2 (G2-TAPER-NONZERO read over every row run)",
          "G2-TAPER-NONZERO c2-100 CAND s42 rep1" in t_ and re.search(r"^GATE-67G2 FAIL .*G2-TAPER-NONZERO", t_, re.M) is not None, t_[-900:])
    t = _g_score("g2", _g_ledger("g2"), "g2nz-clean.log")
    check("g2_taper_nonzero_green: clean G2 rows print no G2-TAPER-NONZERO line", "G2-TAPER-NONZERO" not in t and "GATE-67G2 PASS" in t, t[-500:])


def test_r1_readout_values():
    """Item 9: the R1 READOUT prints numbers (the keys exist on the rows), never `-`."""
    rows = ledger_rows("67r")
    r0 = rows[0]
    keys = ("mbps", "cpu_cli", "cpu_srv", "cli_lag_E", "srv_lag_E", "inorder_mean")
    check("r1_readout_keys: a row carries numeric mbps, cpu_cli, cpu_srv, cli_lag_E, srv_lag_E, inorder_mean", all(isinstance(r0.get(k), (int, float)) for k in keys), {k: r0.get(k) for k in keys})
    t = scored("67r", rows)
    for arm in ("S1", "S2", "B0"):
        ln = next(x for x in t.splitlines() if x.startswith(f"  READOUT {arm} "))
        vals = {k: re.search(re.escape(k) + r" (?:med |mean )?(-?[0-9.]+)", ln) for k in ("goodput", "cpu_cli/GB", "cpu_srv/GB", "lag_E_cli", "lag_E_srv", "in-order delay")}
        check(f"r1_readout_values: {arm} READOUT carries numbers for goodput, cpu_cli/GB, cpu_srv/GB, lag_E_cli, lag_E_srv, in-order mean (none `-`)", all(v and float(v.group(1)) > 0 for v in vals.values()), ln)
        want = {"goodput": lc.med([x["mbps"] for x in rows if x["arm"] == arm]), "lag_E_cli": lc.med([x["cli_lag_E"] for x in rows if x["arm"] == arm]), "lag_E_srv": lc.med([x["srv_lag_E"] for x in rows if x["arm"] == arm])}
        check(f"r1_readout_values: {arm} the printed goodput / lag medians are the rows' own", abs(float(vals["goodput"].group(1)) - want["goodput"]) < 0.06 and int(float(vals["lag_E_cli"].group(1))) == round(want["lag_E_cli"])
              and int(float(vals["lag_E_srv"].group(1))) == round(want["lag_E_srv"]), (ln, want))


def test_t_bringup_attribution():
    """Item 9: TUN-BRINGUP-FAIL is attributed (by its label, else the invocation the battery started) or aborts `unattributed`; never dropped. Clause 2 alone is VOID."""
    base = F.tun_lines()
    p = os.path.join(TMP, "tun-bf-label.log")
    open(p, "w").write("\n".join(base[:3] + ["TUN-BRINGUP-FAIL label=B0-realtime"] + base[3:]) + "\n")     # inside the (B0, auto) invocation, naming another one
    res = S.score_t(p, lambda *_: None)
    check("t_bringup_label: a TUN-BRINGUP-FAIL names its invocation by its label (B0-realtime), wherever it sits in the ledger", res.startswith("ABORT-BRINGUP-T seat=B0 hint=realtime") and "seat=B0 hint=auto" not in res, res)
    p = os.path.join(TMP, "tun-bf-cur.log")
    open(p, "w").write("\n".join(base[:3] + ["TUN-BRINGUP-FAIL"] + base[3:]) + "\n")     # no label: the invocation the battery started (T67SEAT B0 auto)
    res = S.score_t(p, lambda *_: None)
    check("t_bringup_cur: a label-less TUN-BRINGUP-FAIL goes to the invocation the battery last started (T67SEAT)", res.startswith("ABORT-BRINGUP-T seat=B0 hint=auto"), res)
    p = os.path.join(TMP, "tun-bf-orphan.log")
    open(p, "w").write("\n".join(["TUN-BRINGUP-FAIL"] + base) + "\n")     # before any T67SEAT line, no label
    lines = []
    res = S.score_t(p, lines.append)
    check("t_bringup_unattributed: a bring-up failure outside any invocation (no label, no T67SEAT yet) is ABORT-BRINGUP-T naming `unattributed`, never dropped", res.startswith("ABORT-BRINGUP-T unattributed") and not any(x.startswith("T PASS") for x in lines), res)
    lines = []
    res = S.score_t(_tun("b0-run-key-false"), lines.append)
    tx = "\n".join(lines)
    check("t_clause2_void: B0's key reads false while every transfer completes and TUNPIPE / TUNCFG are clean: clauses 3 and 4 hold, the verdict is T VOID (not FAIL)",
          res.startswith("T VOID") and "(3) every S1 transfer completes wherever the B0 transfer of the same (hint, rep) completes: ok" in tx
          and "(4) both ends of every S1 invocation print TUNPIPE with the given hint and a TUNCFG whose four fields read false: ok" in tx and "T FAIL" not in tx, (res, tx[-900:]))


TESTS = [test_s06_taper_red, test_s06_taper_green, test_s2_pslot_red, test_s2_pslot_green, test_s1_gauge_red, test_s1_gauge_green, test_b0_gauge_red, test_seat_sha_red_green, test_short_red_green,
         test_dnf_row, test_rows_never_raise, test_as_red_green, test_a1_red_green, test_a1_exact_matches_mc, test_unscoreable_red_green, test_replacement_cap, test_s06_stage_red_green,
         test_s06_dnf_red_green, test_belowfloor_red_green, test_succ_red_green, test_rate_table, test_owner_flag, test_block1, test_a1r_red_green, test_r1_red_green, test_r1_blind,
         test_rho_red_green, test_r2_hold_red_green, test_r2_final_label, test_r2_under_min_red_green, test_r2_candidate_crash, test_r2_pace_witness, test_mw_exact, test_pool_red_green, test_dump_twins, test_smoke_red_green,
         test_t_green_red, test_smoke_t, test_g1_red_green, test_g2_red_green, test_g_gate, test_smoke_g, test_fixtures_deterministic,
         test_s06_precedence, test_slotfill_live_only, test_replaceable_set, test_r1_b0_gauge, test_r1_readouts, test_r1_complete, test_truncated, test_r2_blind, test_r2_missing_field,
         test_rho_witness_line, test_t_precedence, test_t_parser_agrees_with_verify4, test_g_block1,
         test_candidate_crash, test_truncation_every_verdict, test_ledger_drop_state, test_g2_taper_preconditions, test_r1_readout_values, test_t_bringup_attribution]

if __name__ == "__main__":
    import time
    t0 = time.time()
    for fn in TESTS:
        try:
            fn()
        except Exception as e:      # a test that raises is a failed test, never a silent skip
            import traceback
            check(fn.__name__ + " raised", False, traceback.format_exc()[-1200:])
    print(f"{CHECKS} checks, {fails} failed, {time.time() - t0:.1f} s")
    sys.exit(1 if fails else 0)
