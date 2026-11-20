#!/usr/bin/env python3
"""Tests for plan 67's scorer (s67_score.py) and its synthetic log shapes (s67_fixtures.py). One clause = one function per outcome: test_<clause>_red (the defect: must read FAIL / HOLD / VOID ...) and
test_<clause>_green (the control). The rows are the REAL row parser's rows: the committed fixture log (docs/l1-raw/thread-pd/battery/diag-smoke) stripped by s67_fixtures.strip_base plus s67_fixtures.extra_lines.
Run: python3 test_s67.py   (about half a minute)"""
import json
import os
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
    check("a_under_min_red: fewer than MIN_LIVE_A live S2 rows in a cell -> AS and A1 UNSCOREABLE (never a PASS)", "AS UNSCOREABLE" in t and "A1 UNSCOREABLE" in t, t[-700:])
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
def g_make(which, cell, arm, seed, rep, defect=None, rc=0):
    """What place38_parse.make_row does with the hooks (LD_PLAN=41d, LD_WITNESS67=<which>): make_row, then g_row's problems merged, then g_status."""
    key = ("g", which, cell, arm, str(seed), int(rep), defect, rc)
    if key in _CACHE:
        return json.loads(json.dumps(_CACHE[key]))
    seat = F.G_SEAT[which][arm]
    cli, srv, drv = logs(which, cell, arm, seed, rep, defect)
    os.environ["LD_PLAN"] = "41d"
    os.environ["LD_ROW_BYTES"] = str(F.RUN_BYTES[cell])
    try:
        sha, shac, shab = "c" * 64, "c" * 64, "d" * 64
        row = pp.make_row(cell, arm, seed, rep, rc, "60", drv, cli, srv, 0, shac if arm == "CAND" else shab)
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
    check("g2_taper_red: an S2 row with taper > 0 is S06-FAIL, keeps its 41d label, is not replaceable", r["status"] == "S06-FAIL" and r["status_41d"] == "LIVE" and "S06-FAIL" not in pp.REPLACEABLE, (r["status"], r["problems"]))
    check("g2_taper_red: view41d scores it under its 41d label", S.view41d_g(r)["status"] == "LIVE")
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


def test_fixtures_deterministic():
    a = F.extra_lines("67a", "c2-100", "S1", "42", 1)
    b = F.extra_lines("67a", "c2-100", "S1", "42", 1)
    check("fixtures_green: extra_lines is deterministic in (plan, cell, arm, seed, rep)", a == b and a != F.extra_lines("67a", "c2-100", "S1", "42", 2))
    check("fixtures_green: every defect name is accepted", all(F.extra_lines("67a", "c2-100", "S2", "7", 2, d) for d in sorted(F.DEFECTS) if d not in ("twin-no-final", "twin-under-min", "twin-worse", "twin-no-acct", "no-pace-echo", "r1-blind", "bulk-rr")))


TESTS = [test_s06_taper_red, test_s06_taper_green, test_s2_pslot_red, test_s2_pslot_green, test_s1_gauge_red, test_s1_gauge_green, test_b0_gauge_red, test_seat_sha_red_green, test_short_red_green,
         test_dnf_row, test_rows_never_raise, test_as_red_green, test_a1_red_green, test_a1_exact_matches_mc, test_unscoreable_red_green, test_replacement_cap, test_s06_stage_red_green,
         test_s06_dnf_red_green, test_belowfloor_red_green, test_succ_red_green, test_rate_table, test_owner_flag, test_block1, test_a1r_red_green, test_r1_red_green, test_r1_blind,
         test_rho_red_green, test_r2_hold_red_green, test_r2_final_label, test_r2_under_min_red_green, test_r2_pace_witness, test_mw_exact, test_pool_red_green, test_dump_twins, test_smoke_red_green,
         test_t_green_red, test_smoke_t, test_g1_red_green, test_g2_red_green, test_g_gate, test_smoke_g, test_fixtures_deterministic]

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
