#!/usr/bin/env python3
"""Tests for the merge guard v2 (status 41d): the clause-set version, the in-order delay clause and its edge cases, the VOID rule, the plan-41d row parser and
scorer, v1 staying bit-identical, and the reproduction of the dry run on the committed ledgers (docs/derivations/guard-clause-dry.md).

  A  clause sets (v1 default, v2, a dict as data)        B  the in-order delay definition's edge cases
  C  the row parser under LD_PLAN=41d: VOID-SHORT        D  the scorer on synthetic ledgers (what v2 sees, what it does not)
  E  the committed ledgers: v1 reproduces p = 0.2471 (S52) and 0.0020 (S48b) exactly; the dry run's splice; the accepted v2 set on S48b
  F  the harness copies of the sizes agree

Run: python3 test_guard_v2.py   (E scores the real ledgers at 10 000 relabellings: about a minute each)"""
import contextlib
import io
import json
import os
import random
import re
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import l1common as lc  # noqa: E402
import place38_parse as pp  # noqa: E402
import place41b_score as g  # noqa: E402
import stubrun_gen as sg  # noqa: E402
import threadp1_parse as t1  # noqa: E402

fails = 0
RAW = os.path.join(HERE, "..", "..", "docs", "l1-raw")
DRY = os.path.join(HERE, "..", "..", "docs", "derivations", "guard-clause-dry")


def check(name, ok, detail=""):
    global fails
    print(("ok   " if ok else "FAIL ") + name + ((" -- " + str(detail)[:1500]) if not ok else ""))
    if not ok:
        fails += 1


def near(a, b, tol=1e-9):
    return a is not None and abs(a - b) <= tol


# ── A: clause sets ───────────────────────────────────────────────────────────────────────────────────────
V1 = {"c1s-400": ["goodput", "cpu_cli/GB", "cpu_srv/GB", "lag_E_cli", "lag_E_srv", "rtp_floor_p0"],
      "c1d-400": ["goodput", "cpu_cli/GB", "cpu_srv/GB", "lag_E_cli", "lag_E_srv", "rtp_floor_p0", "rtp_floor_p1"],
      "c2-100": ["goodput", "cpu_cli/GB", "cpu_srv/GB", "lag_E_cli", "lag_E_srv", "rtp_floor_p0"],
      "c8-100": ["goodput", "cpu_cli/GB", "cpu_srv/GB", "lag_E_cli", "lag_E_srv", "rtp_floor_p0", "rtp_floor_p1"]}
names = lambda cell, cs="v1": [n for n, _, _ in g.clause_defs(cell, cs)]  # noqa: E731
check("v1 is the default: clause_defs(cell) and clause_defs(cell, 'v1') give status 47's clause set at every cell, 26 clauses over the four",
      all(names(c) == V1[c] == names(c, "v1") for c in V1) and sum(len(v) for v in V1.values()) == 26, {c: names(c) for c in V1})
V2 = {"c1s-400": V1["c1s-400"][:5], "c1d-400": V1["c1d-400"][:5], "c2-100": V1["c2-100"][:5] + ["inorder_delay_mean"], "c8-100": V1["c8-100"][:5] + ["inorder_delay_mean"]}
check("v2: rtp_floor_p0/p1 scored nowhere (c1s, c1d too), inorder_delay_mean scored at c2 and c8 only, every other clause unchanged (22 clauses)",
      all(names(c, "v2") == V2[c] for c in V2) and sum(len(v) for v in V2.values()) == 22, {c: names(c, "v2") for c in V2})
check("the new clause is one-sided, higher is worse (sign +1), read from the row key inorder_mean (ms); the other clauses keep their keys and signs",
      {n: (k, s) for n, k, s in g.clause_defs("c8-100", "v2")}["inorder_delay_mean"] == ("inorder_mean", 1)
      and [(n, k, s) for n, k, s in g.clause_defs("c8-100", "v2")][:5] == [(n, k, s) for n, k, s in g.clause_defs("c8-100")][:5])
splice = {"floor": ("c1s-400", "c1d-400"), "inorder": ("c2-100", "c8-100")}
check("a clause set is data: a dict {floor: cells, inorder: cells} is accepted (the dry run's splice keeps the floor at c1 only)",
      names("c1d-400", splice) == V1["c1d-400"] and names("c2-100", splice) == V2["c2-100"] and names("c8-100", {"floor": None, "inorder": ()}) == V1["c8-100"])
try:
    g.clause_defs("c8-100", "v3")
    check("an unknown version name raises KeyError", False)
except KeyError:
    check("an unknown version name raises KeyError", True)

# ── B: the in-order delay definition (status 41d item 3) ─────────────────────────────────────────────────


def diag(t, src, cum):
    s, c, a = cum
    return f"[DIAG] t={t:.1f}s win=1/1 src={src}sym/s cod=0sym/s cum={s}/{c}/{a} cwnd=1 infl=1"


# sent curve 100/s; the ack lags by 1 s from t=3; ticks at t = 1..6; the source stops after t=4
LB = [diag(1.0, 5, (100, 0, 0)), diag(2.0, 5, (200, 0, 0)), diag(3.0, 5, (300, 0, 100)), diag(4.0, 5, (400, 0, 300)), diag(5.0, 0, (400, 0, 350)), diag(6.0, 0, (400, 0, 400))]
d = dict(t1.inorder_delay(LB))
check("ticks before the first ack (ack = 0 at t = 1, 2) are excluded; delay(t) = t - t_src(ack(t)): t=3 -> 3-1 = 2.0 s, t=4 -> 4-3 = 1.0 s",
      set(d) == {3.0, 4.0, 5.0, 6.0} and near(d[3.0], 2.0) and near(d[4.0], 1.0), d)
check("t_src is the inverse of the cumulative SOURCE curve, linearly interpolated: ack 350 at t = 5 -> t_src = 3.5 (curve passes 300 at t=3, 400 at t=4) -> delay 1.5 s",
      near(d[5.0], 1.5) and near(d[6.0], 6.0 - 4.0), d)
check("an ack beyond the last source tick's cumulative is excluded, never clamped",
      dict(t1.inorder_delay(LB[:4] + [diag(5.0, 0, (400, 0, 450))])).get(5.0) is None)
st = t1.inorder_delay_stats(LB)
check("the mean is over ticks with ack > 0 (4 ticks here): (2.0 + 1.0 + 1.5 + 2.0) / 4", near(st["mean"], 6.5 / 4) and st["n"] == 4, st)
check("a log with no ack > 0 tick has no in-order delay (None: an undefined row, never 0)", t1.inorder_delay_stats([diag(1.0, 5, (100, 0, 0))]) is None)
check("last_tick_s is the time of the last [DIAG] tick (0 with none)", t1.last_tick_s(LB) == 6.0 and t1.last_tick_s(["x"]) == 0.0)

# ── C: the row parser under LD_PLAN=41d ──────────────────────────────────────────────────────────────────
PD = os.path.join(RAW, "thread-pd", "battery")
FIXD = {"single": "diag-smoke/c1s-400-MAIN-s42-r1-a1-", "dual": "diag-tpd/c1d-400-MAIN-s42-r1-a1-"}
FAM = {"c1s-400": "c1s", "c1d-400": "c1d", "c2-100": "c2", "c8-100": "c8"}


def mk(cell, arm, ticks, plan="41d", dnf=False, nbytes="600000000", ack=True):
    """A parsed row of `cell`/`arm` whose client log carries `ticks` [DIAG] ticks (0.25 s apart) with src= and cum= (the stub driver's shape)."""
    mode = "dual" if cell in ("c1d-400", "c8-100") else "single"
    drv, cli, srv = (lc.read(os.path.join(PD, FIXD[mode] + x)) for x in ("drv.out", "c.log", "s.log"))
    cli = [ln for ln in cli if "[DIAG] t=" not in ln and "[ETA] site=sender" not in ln and sg.STOCK_ECHO not in ln and sg.BRS_ECHO not in ln]
    cli = [re.sub(r"\sRWM_PLACE_WIRE=\S+", "", ln) if "[GATES]" in ln else ln for ln in cli]
    srv = [re.sub(r"\sRWM_PLACE_WIRE=\S+", "", ln) if "[GATES]" in ln else ln for ln in srv]
    egress = [500, 500] if mode == "dual" else [1000]
    drv = drv + [f"    [TRUTH] leg={i} dev=cli{i} egress_dgrams={e} egress_skbs=10 gso=1.00 netem_sent_dgrams={e} netem_dropped_skbs=0 backlog=0 lost=5 loss=0.005000 rcvbuf_drops=0 rcvbuf_scope=netns"
                 for i, e in enumerate(egress)]
    if dnf:
        cli = cli + ['{"run":1,"dnf":true}']          # the perf client's per-run line of a run that did not finish
    rl = sg.run_lines(FAM[cell], False, n=ticks)
    k = 0
    for i, ln in enumerate(rl):
        if ln.startswith("[DIAG]"):
            k += 1
            rl[i] = ln + sg.extras52(k, n_src=max(1, ticks - 20))
    if not ack:                                           # no tick ever has ack > 0
        rl = [re.sub(r"cum=(\d+)/(\d+)/\d+", r"cum=\1/\2/0", ln) for ln in rl]
    env = {"LD_ROW_BYTES": nbytes} if nbytes else {}
    if plan:
        env["LD_PLAN"] = plan
    old = {k_: os.environ.get(k_) for k_ in ("LD_PLAN", "LD_ROW_BYTES")}
    for k_ in old:
        os.environ.pop(k_, None)
    os.environ.update(env)
    try:
        return pp.make_row(cell, arm, "42", 1, 0, 60.0, drv, cli + rl, srv, 0, "x", store="def")
    finally:
        for k_, v in old.items():
            os.environ.pop(k_, None)
            if v is not None:
                os.environ[k_] = v


r = mk("c8-100", "BASE", 200)
check("41d: a 600 MB c8 row with 50 s of ticks is LIVE and carries inorder_mean (ms), inorder_p95, inorder_n, tick_span_s and its own byte count",
      r["status"] == "LIVE" and r["inorder_mean"] > 0 and r["inorder_p95"] >= r["inorder_mean"] * 0 and r["inorder_n"] > 100 and r["tick_span_s"] >= 49.9 and r["bytes"] == 600000000,
      (r["status"], r["problems"], r.get("inorder_mean"), r.get("tick_span_s")))
ref_lines = [ln + sg.extras52(k + 1, n_src=180) for k, ln in enumerate(l_ for l_ in sg.run_lines("c8", False, n=200) if l_.startswith("[DIAG]"))]
mean_ms = t1.inorder_delay_stats(ref_lines)["mean"] * 1e3
check("... and inorder_mean is inorder_delay_stats(cli)['mean'] x 1000 (ms)", near(r["inorder_mean"], mean_ms, 1e-6), (r["inorder_mean"], mean_ms))
r = mk("c8-100", "BASE", 200, plan=None, nbytes=None)
check("v1 rows are untouched: without LD_PLAN=41d the row has no inorder / tick_span fields", "inorder_mean" not in r and "tick_span_s" not in r and r["status"] == "LIVE", sorted(k for k in r if "inorder" in k or "tick" in k))
r = mk("c8-100", "BASE", 200, plan="52")
check("... and under another plan (52) neither", "inorder_mean" not in r and "tick_span_s" not in r)
r = mk("c1d-400", "BASE", 200, nbytes=None)
check("41d: a c1 row carries neither (the clause is not scored at c1)", "inorder_mean" not in r and "tick_span_s" not in r and r["status"] == "LIVE", r["status"])
for cell in ("c2-100", "c8-100"):
    r = mk(cell, "BASE", 40)
    check(f"VOID rule: a {cell} row with 10 s of ticks (< 30 s) is VOID-SHORT with the reason in its problems, never LIVE",
          r["status"] == "VOID-SHORT" and any("short-row" in p for p in r["problems"]) and r["tick_span_s"] < 30, (r["status"], r["problems"]))
r = mk("c8-100", "BASE", 119)       # last tick 29.75 s
check("... the bound is 30 s on the last tick: a last tick at 29.8 s is VOID-SHORT", r["status"] == "VOID-SHORT" and r["tick_span_s"] < 30.0, (r["status"], r["tick_span_s"]))
r = mk("c8-100", "BASE", 120)       # last tick 30.0 s
check("... and 30.0 s is LIVE (the bound is inclusive)", r["status"] == "LIVE" and r["tick_span_s"] >= 30.0, (r["status"], r["tick_span_s"], r["problems"]))
r = mk("c8-100", "BASE", 200, ack=False)
check("VOID rule: a long row with no tick of ack > 0 (no in-order delay at all) is VOID-SHORT, not a neutral fill", r["status"] == "VOID-SHORT" and r["inorder_mean"] is None and "no-inorder-delay-tick" in r["problems"], (r["status"], r["problems"]))
r = mk("c8-100", "CAND", 40, dnf=True)
check("a DNF row is NOT voided by the short-row rule: it stays LIVE + dnf (the worst rank on every clause), so a candidate that dies early gets no free re-run",
      r["status"] == "LIVE" and r["dnf"] is True, (r["status"], r["dnf"], r["problems"]))
base_void = mk("c8-100", "BASE", 40)
ok_row = dict(mk("c8-100", "BASE", 200), seed="7")
base_void["seed"] = "7"
rows_, info_ = pp.apply_replacements([base_void, ok_row])
check("VOID-SHORT is replaceable: the slot's second row takes it (counted against the cap of 4), and an unreplaced one stays voided",
      "VOID-SHORT" in pp.REPLACEABLE and [x["status"] for x in rows_] == ["LIVE"] and info_["void_slots"] == [("c8-100", "BASE", "7", "1")]
      and pp.apply_replacements([base_void])[1]["void_slots"] == [("c8-100", "BASE", "7", "1")] and pp.apply_replacements([base_void])[0][0]["status"] == "VOID-SHORT")


# ── D: the scorer on synthetic ledgers ───────────────────────────────────────────────────────────────────
def synth(seed=1, shift_inorder=None, shift_floor=None, drop_inorder_at=None):
    """A 96-row ledger in the guard's design: BASE and CAND 12 each per cell, 6 + 6 per seed. shift_*: {cell: factor} on CAND."""
    rng = random.Random(seed)
    rows = []
    for cell, _ in g.CELLS_GUARD:
        long_ = cell in g.LONG_CELLS
        for arm in ("BASE", "CAND"):
            for sd in ("42", "7"):
                for rep in range(1, 7):
                    f_in = (shift_inorder or {}).get(cell, 1.0) if arm == "CAND" else 1.0
                    f_fl = (shift_floor or {}).get(cell, 1.0) if arm == "CAND" else 1.0
                    r = {"cell": cell, "store": "def", "arm": arm, "seed": sd, "rep": rep, "status": "LIVE", "dnf": False, "rc": 0, "problems": [],
                         "mbps": rng.gauss(800, 8), "cpu_cli": rng.gauss(6.5, 0.1) * (0.6 if long_ else 0.4), "cpu_srv": rng.gauss(8.6, 0.1) * (0.6 if long_ else 0.4),
                         "cli_lag_E": rng.gauss(900, 20), "srv_lag_E": rng.gauss(900, 20), "bytes": 600000000 if long_ else None, "io_high": False}
                    for i in range(pp.sp.N_LEGS[cell]):
                        r[f"rtp_floor_p{i}"] = rng.gauss(2400, 40) * f_fl
                    if long_:
                        r["inorder_mean"], r["inorder_p95"], r["tick_span_s"], r["inorder_n"] = rng.gauss(4000, 120) * f_in, 6000.0, 48.0, 190
                        if drop_inorder_at == (cell, arm, sd, rep):
                            r["inorder_mean"] = None
                    rows.append(r)
    return rows


NP = 2000
aa = synth()
s1, s2 = g.statistic_rank(aa, "BASE", "CAND", n_perm=NP, cells=g.CELLS_GUARD), g.statistic_rank(aa, "BASE", "CAND", n_perm=NP, cells=g.CELLS_GUARD, clause_set="v2")
check("synthetic A/A ledger: v1 and v2 both PASS (p > 0.05), v2 has 22 clauses over the four cells and v1 26",
      s1["p"] > 0.05 and s2["p"] > 0.05 and len(s1["p_true"]) == 26 and len(s2["p_true"]) == 22, (s1["p"], s2["p"], len(s1["p_true"]), len(s2["p_true"])))
sh = synth(shift_inorder={"c8-100": 1.4})
v1, v2 = g.statistic_rank(sh, "BASE", "CAND", n_perm=NP, cells=g.CELLS_GUARD), g.statistic_rank(sh, "BASE", "CAND", n_perm=NP, cells=g.CELLS_GUARD, clause_set="v2")
check("c8 in-order delay +40 % on CAND: v2 FAILS at inorder_delay_mean (the clause the guard lacked), v1 does not see it",
      v2["p"] <= 0.01 and v2["argmin"] == ("c8-100", "inorder_delay_mean") and v1["p"] > 0.05, (v1["p"], v2["p"], v2["argmin"]))
sh = synth(shift_floor={"c8-100": 3.0, "c2-100": 3.0, "c1s-400": 3.0})
v1, v2 = g.statistic_rank(sh, "BASE", "CAND", n_perm=NP, cells=g.CELLS_GUARD), g.statistic_rank(sh, "BASE", "CAND", n_perm=NP, cells=g.CELLS_GUARD, clause_set="v2")
check("the start-up floor x3 on CAND at c1s / c2 / c8: v1 FAILS on it, v2 does not score it (p > 0.05) -- reported only everywhere, c1 included",
      v1["p"] <= 0.05 and "rtp_floor" in v1["argmin"][1] and v2["p"] > 0.05, (v1["p"], v1["argmin"], v2["p"]))
check("a v1 scorer call without a clause_set is the v1 call (default), bit for bit", g.statistic_rank(sh, "BASE", "CAND", n_perm=NP, cells=g.CELLS_GUARD, clause_set="v1")["p"] == v1["p"])
miss = synth(drop_inorder_at=("c8-100", "CAND", "7", 3))
check("long_row_problems: a LIVE long-cell row without inorder_mean is listed under v2 (it would be a neutral rank fill), nothing under v1",
      len(g.long_row_problems(miss, "v2")) == 1 and "c8-100 CAND s7 r3" in g.long_row_problems(miss, "v2")[0] and g.long_row_problems(miss, "v1") == [])
short = synth()
for r in short:
    if (r["cell"], r["arm"], r["seed"], r["rep"]) == ("c2-100", "BASE", "42", 2):
        r["tick_span_s"] = 12.0
check("... and so is a LIVE long-cell row with under 30 s of ticks; DNF rows are not (they are the worst rank, not a fill)",
      len(g.long_row_problems(short, "v2")) == 1 and g.long_row_problems([dict(r, dnf=True, inorder_mean=None) for r in short], "v2") == [])


def ledger(rows):
    fd, path = tempfile.mkstemp(suffix=".log", prefix="guardv2-")
    with os.fdopen(fd, "w") as fh:
        fh.write("\n".join("P38ROW " + json.dumps(r, sort_keys=True) for r in rows) + "\nLD-BATTERY-DONE tag=ld\n")
    return path


def run_score(fn, rows):
    p = ledger(rows)
    buf = []
    try:
        res = fn([p], out=buf.append)
    finally:
        os.remove(p)
    return res, "\n".join(buf)


res, txt = run_score(pp.score41d, synth())
check("score41d on a complete synthetic A/A ledger: the v2 title, 22 scored clauses incl. both inorder_delay_mean, the reported-only block, 'VERDICT GATE-41d PASS'",
      "MERGE-CANDIDATE GUARD v2" in txt and "VERDICT GATE-41d PASS" in txt and "c2-100 inorder_delay_mean" in txt and "c8-100 inorder_delay_mean" in txt
      and "REPORTED ONLY (not scored" in txt and "rtp_floor_p0 " in txt.split("REPORTED ONLY")[1] and "inorder_p95" in txt.split("REPORTED ONLY")[1], txt[-1500:])
res, txt = run_score(pp.score41d, synth(drop_inorder_at=("c8-100", "CAND", "7", 3)))
check("score41d with one LIVE c8 row lacking the in-order mean: VERDICT NO VERDICT (never PASS), the row named", "VERDICT NO VERDICT, re-run" in txt and "c8-100 CAND s7 r3" in txt and "GATE-41d PASS" not in txt.split("VERDICT")[-1], txt[-800:])
void = synth()
void = [r for r in void if (r["cell"], r["arm"], r["seed"], r["rep"]) != ("c8-100", "BASE", "42", 4)]
void.append({"cell": "c8-100", "store": "def", "arm": "BASE", "seed": "42", "rep": 4, "status": "VOID-SHORT", "dnf": False, "rc": 0, "problems": ["short-row last-tick=10.0s (< 30 s)"], "io_high": False,
             "mbps": 800.0, "cpu_cli": 1.0, "cpu_srv": 1.0, "cli_lag_E": 1.0, "srv_lag_E": 1.0})
res, txt = run_score(pp.score41d, void)
check("score41d with a VOID-SHORT slot that was not re-run: the slot is missing from the design, so NO VERDICT (and the status table counts it)",
      "VERDICT NO VERDICT" in txt and "VOID-SHORT" in txt and "INCOMPLETE DESIGN" in txt, txt[-800:])
res, txt = run_score(pp.score41c, synth())
check("score41c (v1) on the same shape of ledger is unchanged in form: v1 title, 26 clauses, GATE-41c, no REPORTED ONLY block, no VOID-SHORT row in the status table",
      "MERGE-CANDIDATE GUARD (status 41b rule, status 47 harness)" in txt and "VERDICT GATE-41c" in txt and "REPORTED ONLY" not in txt and "VOID-SHORT" not in txt and "inorder" not in txt, txt[-600:])


# ── E: the committed ledgers ─────────────────────────────────────────────────────────────────────────────
def load(ld, rowvals=None):
    rows, _ = pp.rows_of([ld])
    rows, _ = pp.apply_replacements(rows)
    if rowvals:
        rv = json.load(open(rowvals))
        for r in rows:
            v = rv.get("|".join([r["cell"], r["arm"], str(r["seed"]), str(r["rep"])]))
            r["inorder_mean"] = None if not v or not v.get("B") else v["B"]["mean"] * 1e3         # the in-order means of the dry run (diag logs, ms)
    return rows


def pt(res):
    return {"%s|%s" % k: v for k, v in res["p_true"].items()}


def same_ptrue(res, ref):
    a, b = pt(res), ref["p_true"]
    return set(a) == set(b) and all(near(a[k], b[k], 1e-12) for k in a)


LD52 = os.path.join(RAW, "s52", "guard-ld.log")
LD48 = os.path.join(RAW, "bbr48b", "run", "bbr48", "ld.log")
RV48 = os.path.join(DRY, "rowvals-bbr48b.json")
r52, r48 = load(LD52), load(LD48, RV48)
js = lambda f: json.load(open(os.path.join(DRY, f)))  # noqa: E731
res = g.statistic_rank(r52, "BASE", "CAND", cells=g.CELLS_GUARD)
check("v1 reproduces S52's guard exactly: p = 0.2471 (min-p 0.01320 at c1s cpu_cli/GB), every clause p equal to the dry run's 'old' set",
      near(res["p"], 0.2471, 5e-5) and near(res["p"], js("set-s52-old.json")["p"], 1e-12) and same_ptrue(res, js("set-s52-old.json")) and res["argmin"] == ("c1s-400", "cpu_cli/GB"), (res["p"], res["argmin"]))
res = g.statistic_rank(r48, "BASE", "CAND", cells=g.CELLS_GUARD)
check("v1 reproduces S48b exactly: p = 0.0020 (min-p 0.0001 at c2 goodput), every clause p equal to the dry run's 'old' set",
      near(res["p"], 0.0020, 5e-5) and near(res["p"], js("set-bbr48b-old.json")["p"], 1e-12) and same_ptrue(res, js("set-bbr48b-old.json")) and res["argmin"] == ("c2-100", "goodput"), (res["p"], res["argmin"]))
res = g.statistic_rank(r52, "BASE", "CAND", cells=g.CELLS_GUARD, clause_set={"floor": ("c1s-400", "c1d-400"), "inorder": ()})
check("the dry run's splice on S52 (floor dropped at c2 / c8 only, no long rows) reproduces its 'new' set: p = 0.2206, every clause p equal",
      near(res["p"], 0.2206, 5e-5) and near(res["p"], js("set-s52-new.json")["p"], 1e-12) and same_ptrue(res, js("set-s52-new.json")), (res["p"],))
res = g.statistic_rank(r48, "BASE", "CAND", cells=g.CELLS_GUARD, clause_set=splice)
check("the dry run's splice on S48b (floor kept at c1, inorder_delay_mean at c2 / c8) reproduces its 'new' set: p = 0.0018, every clause p equal (c2 0.0169, c8 0.0001 for the new clause)",
      near(res["p"], 0.0018, 5e-5) and near(res["p"], js("set-bbr48b-new.json")["p"], 1e-12) and same_ptrue(res, js("set-bbr48b-new.json"))
      and near(res["p_true"][("c2-100", "inorder_delay_mean")], 0.0169, 5e-4) and res["p_true"][("c8-100", "inorder_delay_mean")] < 2e-4, (res["p"], res["p_true"].get(("c2-100", "inorder_delay_mean"))))
res = g.statistic_rank(r48, "BASE", "CAND", cells=g.CELLS_GUARD, clause_set="v2")
check("the ACCEPTED v2 set on S48b (the floor off at c1 too): FAIL, p < 0.05, the min-p clause still c2 goodput; no rtp_floor clause in the family",
      res["p"] < 0.05 and res["argmin"] == ("c2-100", "goodput") and not any("rtp_floor" in k[1] for k in res["p_true"]) and len(res["p_true"]) == 22, (res["p"], res["argmin"]))
print("     (S48b under the accepted v2 set: p = %.4f, min-p %.5f at %s)" % (res["p"], res["T_true"], res["argmin"]))
res52v2 = g.long_row_problems([r for r in r52], "v2")
check("S52's guard has no long rows: under v2 none of its c2 / c8 rows has an in-order mean, so it can never be v2-scored (the scorer would read NO VERDICT, not PASS)",
      len(res52v2) == 24 + 24 and all(r.get("inorder_mean") is None for r in r52), len(res52v2))

# ── F: the harness copies ────────────────────────────────────────────────────────────────────────────────
bat = open(os.path.join(HERE, "loopdiag_battery.sh"), encoding="utf-8").read()
sizes = dict([(c, int(b)) for cs_, b in re.findall(r"p41d_bytes\(\) \{ case \"\$1\" in (\S+)\) echo (\d+) ;;", bat) for c in cs_.split("|")])
tmo = dict([(c, int(b)) for cs_, b in re.findall(r"p41d_timeout\(\) \{ case \"\$1\" in (\S+)\) echo (\d+) ;;", bat) for c in cs_.split("|")])
check("the battery's p41d_bytes / p41d_timeout equal place38_parse.BYTES41D / TIMEOUT41D (c2 and c8 at 600 MB, timeout 200) and plan 48's sizes",
      sizes == pp.BYTES41D == {"c2-100": 600000000, "c8-100": 600000000} and tmo == pp.TIMEOUT41D == {"c2-100": 200, "c8-100": 200}
      and all(pp.BYTES41D[c] == pp.BYTES48[c] for c in pp.BYTES41D), (sizes, tmo))
check("plan 41d is plan 41c's design and order (96 rows, same slots, same rotation); its smoke adds a 600 MB c8 CAND row",
      pp.plan41d() == pp.plan41c() and len(pp.plan41d()) == pp.EXPECT_ROWS41C == 96 and pp.plan41d(smoke=True) == pp.plan41c(smoke=True) + [("c8-100", "def", "CAND", "42", 1)])
check("the v1 plans keep v1: plan 41c's scorer is score41c (no clause_set argument), 48's score48 uses the default clause set",
      "score41c" in open(os.path.join(HERE, "loopdiag_parse.py"), encoding="utf-8").read() and "clause_set" not in open(os.path.join(HERE, "bbr48_score.py"), encoding="utf-8").read())
print("FAILED %d" % fails if fails else "ALL OK")
sys.exit(1 if fails else 0)
