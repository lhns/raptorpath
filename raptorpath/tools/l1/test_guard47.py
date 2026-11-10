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

print("FAILED %d" % fails if fails else "ALL OK")
sys.exit(1 if fails else 0)
