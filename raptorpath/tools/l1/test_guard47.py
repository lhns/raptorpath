#!/usr/bin/env python3
"""Acceptance tests for the guard statistics (status 47, offline): (a) one fully separated clause gives p <= 0.01, (b) A/A relabellings of
BASE-only data fail at about 5 %, plus the mechanics of the rank-minp statistic and the clause inventory (nothing gated reads the [ETA] residual).
The registered 41b statistic (relabel-max) is kept for re-scoring history and is shown NOT to meet (a).  Run: python3 test_guard47.py"""
import os
import random
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import guard_power as gp  # noqa: E402
import place38_parse as pp  # noqa: E402
import place41b_score as g  # noqa: E402

fails = 0


def check(name, ok, detail=""):
    global fails
    print(("ok   " if ok else "FAIL ") + name + ((" -- " + str(detail)) if not ok else ""))
    if not ok:
        fails += 1


pools = gp.load_pools()
check("data: 18 real BASE rows per seed pair at c1s / c1d (9 per seed), 6 main-era rows at c2 / c8 (3 per seed)",
      all(len(pools[c][s]) == 9 for c in ("c1s-400", "c1d-400") for s in gp.SEEDS) and all(len(pools[c][s]) == 3 for c in ("c2-100", "c8-100") for s in gp.SEEDS),
      {c: {s: len(v) for s, v in d.items()} for c, d in pools.items()})

# ── (a) one fully separated clause at one cell: p <= 0.01 ─────────────────
ALL = [(c, cl) for c, _ in gp.CELLS4 for cl in gp.clause_list(c)]
worst, worst_old = 0.0, 1.0
for (c, cl) in [("c1s-400", "cpu_cli/GB"), ("c1d-400", "lag_E_srv"), ("c2-100", "goodput"), ("c8-100", "rtp_floor_p1"), ("c1s-400", "goodput"), ("c8-100", "cpu_srv/GB")]:
    rows = gp.draw_dataset(random.Random(11), pools)
    gp.separate(rows, c, cl)
    pr = g.statistic_rank(rows, "BASE", "CAND", n_perm=10000, cells=gp.CELLS4)["p"]
    worst = max(worst, pr)
    po = g.statistic(rows, "BASE", "CAND", n_perm=2000, cells=gp.CELLS4)["p"]
    worst_old = min(worst_old, po)
check("(a) rank-minp: a fully separated clause (CAND beyond every BASE row) at one cell gives p <= 0.01 at c1s, c1d, c2 and c8 (worst of six cases)", worst <= 0.01, worst)
check("(a) the registered relabel-max statistic does NOT meet it: the same fully separated cases give p well above 0.01 (smallest of the six)", worst_old > 0.01, worst_old)
# a fully separated clause is caught at EVERY clause of the design (all 26), p <= 0.01
bad = []
for (c, cl) in ALL:
    rows = gp.draw_dataset(random.Random(3), pools)
    gp.separate(rows, c, cl)
    p = g.statistic_rank(rows, "BASE", "CAND", n_perm=4000, cells=gp.CELLS4)["p"]
    if p > 0.01:
        bad.append((c, cl, p))
check("(a) rank-minp: all 26 clauses of the four cells, one at a time, p <= 0.01 (N_PERM 4000)", not bad, bad)

# ── (b) A/A: the false-alarm rate is about 5 % ────────────────────────────
ps = [g.statistic_rank(gp.draw_dataset(random.Random(500 + i), pools), "BASE", "CAND", n_perm=1000, cells=gp.CELLS4)["p"] for i in range(240)]
rate = sum(1 for p in ps if p <= 0.05) / len(ps)
check("(b) rank-minp: A/A bootstrap datasets (no shift, four cells) trip p <= 0.05 at about 5 % (0.02 - 0.09 over 240 draws)", 0.02 <= rate <= 0.09, rate)
check("(b) ... and the p values are not piled up at either end (mean in 0.4 - 0.6)", 0.4 <= sum(ps) / len(ps) <= 0.6, sum(ps) / len(ps))
real = []
for i in range(120):
    method, draw, p = gp.w_real(("rank-minp", i, 1000))
    real.append(p)
rate_real = sum(1 for p in real if p <= 0.05) / len(real)
check("(b) rank-minp: exact A/A splits of the 18 real BASE rows of c1s and c1d (3 + 3 of 9 + 9, no resampling) trip at about 5 % (0.01 - 0.11 over 120 splits)", 0.01 <= rate_real <= 0.11, rate_real)

# ── mechanics ─────────────────────────────────────────────────────────────
rows = gp.draw_dataset(random.Random(1), pools)
r1 = g.statistic_rank(rows, "BASE", "CAND", n_perm=1500, cells=gp.CELLS4)
r2 = g.statistic_rank(rows, "BASE", "CAND", n_perm=1500, cells=gp.CELLS4)
check("rank-minp is deterministic (RNG seed 4141) and the p lies in (0, 1]", r1["p"] == r2["p"] and 0 < r1["p"] <= 1 and r1["T_true"] == r2["T_true"])
check("rank-minp covers all 26 clauses of the four cells", len(r1["p_true"]) == 26 and {k[0] for k in r1["p_true"]} == {c for c, _ in gp.CELLS4})
pool = g.Pool(rows, "c1s-400", "BASE", "CAND")
sc = g.rank_scores(pool)
ok = True
for s_, idxs in pool.groups.items():
    got = sorted(sc["cpu_cli/GB"][i] for i in idxs)
    ok = ok and abs(sum(got) - len(idxs) * (len(idxs) + 1) / 2) < 1e-9          # midranks of 9 rows in a seed stratum sum to 45
check("rank scores are within-seed midranks (each seed stratum of 9 rows sums to 45)", ok)
tie = [dict(r) for r in rows]
for r in tie:
    if r["cell"] == "c1s-400":
        r["cpu_cli"] = 5.0
pt = g.Pool(tie, "c1s-400", "BASE", "CAND")
check("ties are midranks: all-equal values give every row the stratum's mean rank (5.0 of 9)", all(abs(v - 5.0) < 1e-9 for v in g.rank_scores(pt)["cpu_cli/GB"]))
check("an all-equal clause has p_k = 1 (ties count as extreme)", abs(g.statistic_rank(tie, "BASE", "CAND", n_perm=500, cells=gp.CELLS4)["p_true"][("c1s-400", "cpu_cli/GB")] - 1.0) < 1e-9)
miss = [dict(r) for r in rows]
miss[0]["cpu_cli"] = None
check("a missing clause value is neutral (mean rank), not an error", g.statistic_rank(miss, "BASE", "CAND", n_perm=300, cells=gp.CELLS4)["p"] > 0)
# one-sidedness: CAND better on a clause is not extreme
better = gp.draw_dataset(random.Random(2), pools)
p_base = g.statistic_rank(better, "BASE", "CAND", n_perm=1500, cells=gp.CELLS4)["p"]
for r in better:
    if r["cell"] == "c2-100" and r["arm"] == "CAND":
        r["mbps"] *= 1.5
        r["cpu_cli"] *= 0.5
check("one-sided: making CAND much better on goodput and CPU (c2) does not lower the p (no flag for better)",
      g.statistic_rank(better, "BASE", "CAND", n_perm=1500, cells=gp.CELLS4)["p"] >= p_base - 0.01)
# shift: a clearly worse CAND is caught, and a small CPU shift in the noise is not
big = gp.draw_dataset(random.Random(4), pools)
gp.shift(big, "c1s-400", "cpu_cli/GB", 0.5)
check("a +50 % client CPU at c1s is caught (p <= 0.01)", g.statistic_rank(big, "BASE", "CAND", n_perm=4000, cells=gp.CELLS4)["p"] <= 0.01)

# ── the report: both methods selectable, history re-scoreable ─────────────
rows41b = pp.rows_of([os.path.join(gp.RAW, "place41b", "ld.log")])[0]
l_old, l_new = [], []
g.report(rows41b, out=l_old.append)
g.report(rows41b, out=l_new.append, method="rank-minp")
check("the registered 41b statistic still re-scores the 41b ledger (GATE-41b PASS p=0.7195)", any("GATE-41b PASS (p=0.7195; the gate-off binary may reach main)" in ln for ln in l_old))
check("the rank-minp statistic re-scores it too and prints per-clause p, the min-p and the gate", any(ln.startswith("GATE-41b[rank-minp] PASS") for ln in l_new) and any("min-p (true)" in ln for ln in l_new))
check("both reports keep the resolution table, the tail line and the run order", all(any("RESOLUTION" in ln for ln in L) and any("  TAIL " in ln for ln in L) and any("RUN ORDER" in ln for ln in L) for L in (l_old, l_new)))

# ── what the gated clauses read (the free check) ──────────────────────────
KEYS = {n: k for n, k, _ in g.clause_defs("c8-100")}
check("the gated clauses are goodput, client / server CPU per GB, lag_E (cli, srv) and the rtp floor per leg - none is an [ETA] field",
      set(KEYS) == {"goodput", "cpu_cli/GB", "cpu_srv/GB", "lag_E_cli", "lag_E_srv", "rtp_floor_p0", "rtp_floor_p1"}
      and set(KEYS.values()) == {"mbps", "cpu_cli", "cpu_srv", "cli_lag_E", "srv_lag_E", "rtp_floor_p0", "rtp_floor_p1"})
src = open(os.path.join(HERE, "place41b_score.py"), encoding="utf-8").read()
body = src[src.index("def clause_defs"):src.index("def med(")]
check("... and place41b_score's clause code never mentions [ETA], e_p50, e_p90 or the residual", not re.search(r"ETA|e_p50|e_p90|residual", body))

# ── DNF / collapse / missing instrument (status 47, red-first) ────────────
def stalled(kind, cell="c1s-400", n_bad=None):
    rows = gp.draw_dataset(random.Random(21), pools)
    k = 0
    for r in rows:
        if r["cell"] == cell and r["arm"] == "CAND":
            if n_bad is not None and k >= n_bad:
                break
            k += 1
            if kind == "dnf-empty":                       # a stalled build: dnf, no goodput, no clause values at all
                r["dnf"] = True
                for key in ("mbps", "cpu_cli", "cpu_srv", "cli_lag_E", "srv_lag_E", "rtp_floor_p0", "rtp_floor_p1"):
                    r[key] = None
            elif kind == "dnf-partial":                   # dnf, but the driver's CPU line and the lag lines were still read
                r["dnf"] = True
                r["mbps"] = None
            elif kind == "collapse":                      # completes, but at a tiny fraction of the cell's goodput
                r["mbps"] = 40.0
    return rows


for kind in ("dnf-empty", "dnf-partial", "collapse"):
    pr = g.statistic_rank(stalled(kind), "BASE", "CAND", n_perm=4000, cells=gp.CELLS4)
    check(f"a stalled CAND build ({kind}: all 6 CAND rows at c1s) FAILS the gate (p <= 0.01): such a row takes the worst rank on every clause of its cell", pr["p"] <= 0.01, pr["p"])
one = g.statistic_rank(stalled("dnf-empty", n_bad=1), "BASE", "CAND", n_perm=4000, cells=gp.CELLS4)
check("one DNF CAND row of six is not by itself a failure (it is the worst rank on every c1s clause, which a single row cannot make significant)", one["p"] > 0.05, one["p"])
base_dnf = stalled("dnf-empty", n_bad=0)
for r in base_dnf:
    if r["cell"] == "c1s-400" and r["arm"] == "BASE" and r["seed"] == "42":
        r["dnf"] = True
        r["mbps"] = None
        break
check("a DNF BASE row is ranked worst too (rank is a property of the row, not its label): the null stays exchangeable and the gate does not fail",
      g.statistic_rank(base_dnf, "BASE", "CAND", n_perm=2000, cells=gp.CELLS4)["p"] > 0.05)
nm = gp.draw_dataset(random.Random(22), pools)
hit = [r for r in nm if r["cell"] == "c2-100" and r["arm"] == "CAND"][0]
hit["cli_lag_E"] = None
res_nm = g.statistic_rank(nm, "BASE", "CAND", n_perm=500, cells=gp.CELLS4)
check("an absent instrument line (a lag_E that could not be read) is neutral, and the neutral fills are COUNTED per clause: c2 lag_E_cli 1, every other clause 0",
      res_nm["neutral"][("c2-100", "lag_E_cli")] == 1 and sum(res_nm["neutral"].values()) == 1 and not any(res_nm["bad"].values()), (res_nm["neutral"], res_nm["bad"]))
dn = stalled("dnf-empty", n_bad=2)
res_dn = g.statistic_rank(dn, "BASE", "CAND", n_perm=500, cells=gp.CELLS4)
check("DNF rows are counted separately from neutral fills (bad rows per cell: 2 at c1s) and are not neutral", res_dn["bad"]["c1s-400"] == 2 and sum(res_dn["neutral"].values()) == 0, (res_dn["bad"], res_dn["neutral"]))
lines = []
g.report(rows41b, out=lines.append, method="rank-minp")
check("the rank-minp report prints the neutral fills and the DNF / collapse rows", any("neutral fills" in ln for ln in lines) and any("worst-ranked rows (DNF = harness 43's COLLAPSE" in ln for ln in lines), [ln for ln in lines if "neutral" in ln][:2])

# ── exclude (lag_E reported-only) and the design check ────────────────────
rx = g.statistic_rank(gp.draw_dataset(random.Random(31), pools), "BASE", "CAND", n_perm=500, cells=gp.CELLS4, exclude=gp.NOLAG)
check("exclude=('lag_E_cli','lag_E_srv') removes the two lag clauses of every cell: 18 clauses left (c1s 4, c1d 5, c2 4, c8 5), none a lag_E",
      len(rx["p_true"]) == 18 and not any(k[1].startswith("lag_E") for k in rx["p_true"]), len(rx["p_true"]))
few = [r for r in gp.draw_dataset(random.Random(32), pools) if not (r["cell"] == "c2-100" and r["arm"] == "CAND" and r["seed"] == "42")]
ri = g.statistic_rank(few, "BASE", "CAND", n_perm=500, cells=gp.CELLS4, design={"c2-100": (6, 6), "c1s-400": (12, 6)})
check("a design check: a cell with fewer live CAND rows than the design is reported INCOMPLETE and can never PASS", ri["incomplete"] == [("c2-100", "CAND", 6, 3)], ri["incomplete"])
li = []
rr = g.report(few, out=li.append, cells=gp.CELLS4, test_arm="CAND", with_wire=False, method="rank-minp", design={"c2-100": (6, 6)})
check("... the report says so and the gate reads NO VERDICT (not PASS) whatever p reads", any("INCOMPLETE DESIGN" in ln for ln in li) and rr["gate"] == "NO VERDICT" and any("NO VERDICT, re-run" in ln for ln in li), (rr["gate"], rr["p"]))

# ── the mandatory design of the merge-candidate guard (red-first) ─────────
D12 = {c: (12, 12) for c, _ in gp.CELLS4}


def gdata(seed=41):
    return gp.draw_dataset(random.Random(seed), pools, design=D12)


def guard_out(rows, design):
    lines = []
    try:
        res = g.report_guard(rows, lines.append, design)
    except Exception as e:           # noqa: BLE001
        return None, lines, e
    return res, lines, None


for bad_design in (None, {}, {"c1s-400": (12, 12)}):
    _, _, err = guard_out(gdata(), bad_design)
    check(f"the guard refuses a missing / empty / partial design ({bad_design!r}): an error, never a silent PASS", isinstance(err, ValueError), err)
res, lines, err = guard_out(gdata(), D12)
check("the guard with the full 12/12 design and a complete A/A dataset gives a verdict (PASS or FAIL by p) and prints no NO VERDICT", err is None and res["gate"] in ("PASS", "FAIL") and not any("NO VERDICT" in ln for ln in lines), (err, res and res["gate"]))
short = [r for r in gdata() if not (r["cell"] == "c8-100" and r["arm"] == "CAND" and r["rep"] == "CAND0" and r["seed"] == "42")]
res, lines, err = guard_out(short, D12)
check("a cell short of one CAND row reads NO VERDICT, re-run (never PASS), whatever p reads", err is None and res["gate"] == "NO VERDICT" and any("NO VERDICT, re-run" in ln for ln in lines) and not any("] PASS" in ln for ln in lines), (err, res and res["gate"]))
voided = gdata()
for r in voided:
    if r["cell"] == "c1s-400" and r["arm"] == "BASE":
        r["status"] = "WITNESS-FAIL"
        break
res, lines, err = guard_out(voided, D12)
check("a voided (non-LIVE) row in a cell leaves the design incomplete: NO VERDICT, re-run", err is None and res["gate"] == "NO VERDICT")
gone = [r for r in gdata() if not (r["cell"] == "c2-100" and r["arm"] == "CAND")]
res, lines, err = guard_out(gone, D12)
check("a cell whose CAND rows never went live (a stalled build: VOID-RC / NO_DATA rows are not LIVE) reads NO VERDICT, never PASS", err is None and res["gate"] == "NO VERDICT")
check("the INCOMPLETE design names the cell, arm and counts", any("INCOMPLETE DESIGN" in ln and "c2-100 CAND 0/12" in ln for ln in lines), [ln for ln in lines if "INCOMPLETE" in ln])
extra = gdata() + [dict(r) for r in gdata()[:2]]
res, lines, err = guard_out(extra, D12)
check("more live rows than the design (a re-run replacing a void one) is fine", err is None and res["gate"] in ("PASS", "FAIL"))
lines = []
import place38_parse as pp2  # noqa: E402
with __import__("tempfile").TemporaryDirectory() as td:
    pth = os.path.join(td, "l.log")
    open(pth, "w").write("".join("P38ROW " + __import__("json").dumps(r) + chr(10) for r in short))
    pp2.score41c([pth], out=lines.append)
check("score41c uses the mandatory-design guard: an incomplete ledger ends in 'VERDICT NO VERDICT, re-run'", lines[-1].startswith("VERDICT NO VERDICT, re-run"), lines[-1])

# ── the session resolution table (red-first): recomputed from THIS session's BASE rows, reported only ──────────
import json as _json  # noqa: E402
import tempfile as _tf  # noqa: E402


def design_ledger(seed=5, cpu_noise=1.0):
    """A 96-row ledger in plan 41c's exact layout (12 rep-major groups, BASE/CAND pairs per cell), bootstrapped from the real single-session rows."""
    rng = random.Random(seed)
    pl = gp.load_pools(single_session=True)
    rows = []
    for (c, st, a, sd, rp) in pp.plan41c():
        src = pl[c][sd]
        r = dict(rng.choice(src))
        r.update({"arm": a, "rep": rp, "seed": sd, "status": "LIVE", "store": "def", "dnf": False, "rc": 0, "problems": [], "problem": None})
        if cpu_noise != 1.0:
            m = {"c1s-400": 15.0 * 0.4, "c1d-400": 21.0 * 0.4, "c2-100": 31.0 * 0.1, "c8-100": 36.0 * 0.1}[c]      # per-GB level x the cell's GB
            r["cpu_cli"] = m + (r["cpu_cli"] - m) * cpu_noise
        rows.append(r)
    return rows


def score41c_text(rows):
    lines = []
    with _tf.TemporaryDirectory() as td:
        pth = os.path.join(td, "l.log")
        open(pth, "w").write("".join("P38ROW " + _json.dumps(r) + chr(10) for r in rows))
        pp.score41c([pth], out=lines.append)
    return lines


g.RES_DRAWS, g.RES_NPERM, g.RES_GRID = 6, 1000, [0.024, 0.05, 0.12, 0.5]
led = design_ledger()
lt = score41c_text(led)
tx = "\n".join(lt)
check("score41c prints the session resolution beside the verdict, marked reported only, for every clause of the four cells (26 lines)",
      "RESOLUTION FROM THIS SESSION'S BASE ROWS (reported only; not part of the verdict)" in tx
      and sum(1 for ln in lt if " d80 = " in ln) == 26 and all(any(f"  {c:<8} " in ln and " d80 = " in ln for ln in lt) for c, _ in gp.CELLS4), [ln for ln in lt if "RESOLUTION FROM" in ln][:1])
check("... with the power at the named shifts (+2.4 % client / server CPU, -5 % goodput) on the CPU and goodput lines",
      sum(1 for ln in lt if "power at 2.4 %" in ln) == 8 and sum(1 for ln in lt if "power at 5 %" in ln) == 4, sum(1 for ln in lt if "power at" in ln))
check("... printed after the guard verdict block and before the final VERDICT line", tx.index("GATE-41b[rank-minp]") < tx.index("RESOLUTION FROM THIS SESSION") < tx.rindex("VERDICT "))
d80 = lambda L, c, n: next(ln for ln in L if ln.startswith(f"  {c:<8} {n:<14} d80 = "))   # noqa: E731
quiet = score41c_text(design_ledger(cpu_noise=0.2))
noisy = score41c_text(design_ledger(cpu_noise=4.0))
pw = lambda ln: float(ln.split("power at 2.4 % = ")[1])   # noqa: E731
check("the resolution is recomputed from THIS session's rows: with a quiet BASE (client CPU noise x0.2) the power at +2.4 % at c1s is higher than with a noisy one (x4)",
      pw(d80(quiet, "c1s-400", "cpu_cli/GB")) > pw(d80(noisy, "c1s-400", "cpu_cli/GB")), (d80(quiet, "c1s-400", "cpu_cli/GB"), d80(noisy, "c1s-400", "cpu_cli/GB")))
few = [r for r in led if not (r["cell"] == "c2-100" and r["arm"] == "BASE" and r["seed"] == "42")] + [r for r in led if r["cell"] == "c2-100" and r["arm"] == "BASE" and r["seed"] == "42"][:1]
lf = score41c_text(few)
check("with fewer than 2 live BASE rows in a (cell, seed) the resolution says so instead of guessing", any("not computed: a (cell, seed) has fewer than 2 live BASE rows" in ln for ln in lf))

print("FAILED %d" % fails if fails else "ALL OK")
sys.exit(1 if fails else 0)
