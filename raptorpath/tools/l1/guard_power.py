#!/usr/bin/env python3
"""Offline acceptance experiments for the status 41b/47 guard statistics (no VM). A guard statistic is accepted only if

  (a) one fully separated clause at any one cell gives p <= 0.01;
  (b) A/A relabellings of BASE-only data fail at about 5 % (the rate over many seeds is reported);
  (c) power from resampled BASE rows: per clause and cell, the smallest shift caught (gate p <= 0.05) in >= 80 % of draws.

Two statistics are compared, both in place41b_score.py: `relabel-max` (the registered 41b statistic: max over clauses of the null-scaled
relative median difference) and `rank-minp` (stratified rank sums, min-p over the same joint relabellings).

    guard_power.py a      [--draws N] [--nperm N]       (a) for every clause of the four guard cells
    guard_power.py b      [--draws N] [--nperm N]       (b) bootstrap A/A at four cells + exact A/A on the 18 real BASE rows of c1s / c1d
    guard_power.py c      [--draws N] [--nperm N] [--cand N]   (c) the power table (CAND n = 6 by default; --cand 12 for the n = 12 variant)
    guard_power.py rescore                               the committed 41 / 41b ledgers re-scored with both statistics
    guard_power.py d      [--draws N] [--nperm N] [--aa N]      (a)(b)(c) at the registered design (CAND 12 / BASE 12 at c1s, c1d; 6 / 6 at c2, c8), single-session rows,
                                                         with all 26 clauses and with lag_E reported-only (18 clauses)

Data. c1s / c1d: the real BASE rows of the 41b ledger (12 per cell) and the 41 ledger (6 per cell), 18 per cell, 9 per seed. c2 / c8: the
only main-era default-law rows are 38's MAIN arm (gate off, 6 per cell, 3 per seed): they stand in for BASE there, so c2 / c8 power rests
on 6 distinct rows per cell (bootstrap with replacement) and is the least trustworthy part of the table.
"""
import copy
import itertools
import json
import multiprocessing as mp
import os
import random
import statistics
import sys
import time
import zlib

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import place38_parse as pp  # noqa: E402
import place41b_score as g  # noqa: E402

RAW = os.path.join(HERE, "..", "..", "docs", "l1-raw")
CELLS4 = g.CELLS_GUARD
SEEDS = ("42", "7")
GRID = [0.01, 0.015, 0.02, 0.024, 0.03, 0.04, 0.05, 0.06, 0.08, 0.10, 0.15, 0.20, 0.30, 0.50, 1.0]
NAMED = {"cpu_cli/GB": 0.024, "cpu_srv/GB": 0.024, "goodput": 0.05}      # the shifts the coordinator asked for explicitly


# The design the guard is registered at (status 47): one interleaved session, balanced per seed. (n_base, n_cand) per cell.
DESIGN = {"c1s-400": (12, 12), "c1d-400": (12, 12), "c2-100": (6, 6), "c8-100": (6, 6)}
if os.environ.get("GUARD_DESIGN"):        # e.g. GUARD_DESIGN='{"c2-100":[8,8]}' overrides the cells it names (design sensitivity runs)
    DESIGN = {**DESIGN, **{c: tuple(v) for c, v in json.loads(os.environ["GUARD_DESIGN"]).items()}}
NOLAG = ("lag_E_cli", "lag_E_srv")


def load_pools(single_session=False):
    """{cell: {seed: [row, ...]}} of real BASE-like rows. single_session=True keeps ONE session per cell (41b's BASE at c1s / c1d, 6 per seed;
    38's MAIN at c2 / c8, 3 per seed) so that no cross-session offset sits in the null."""
    pools = {c: {s: [] for s in SEEDS} for c, _ in CELLS4}
    src = [("place41b", "BASE", ("c1s-400", "c1d-400")), ("place41", "BASE", ("c1s-400", "c1d-400")), ("place38", "MAIN", ("c2-100", "c8-100"))]
    if single_session:
        src = [x for x in src if x[0] != "place41"]
    for d, arm, cells in src:
        rows, _ = pp.rows_of([os.path.join(RAW, d, "ld.log")])
        for r in rows:
            if r["status"] == "LIVE" and r["arm"] == arm and r["cell"] in cells and r["store"] == "def" and not r.get("dnf"):
                pools[r["cell"]][r["seed"]].append(r)
    return pools


def clause_list(cell):
    return [n for n, _, _ in g.clause_defs(cell)]


def row_for(r, arm, tag):
    x = dict(r)
    x["arm"], x["rep"], x["status"], x["store"], x["dnf"] = arm, tag, "LIVE", "def", False
    return x


def draw_dataset(rng, pools, n_base=12, n_cand=6, replace=True, design=None):
    """Bootstrap one guard dataset: per cell and seed n_base/2 BASE rows and n_cand/2 CAND rows resampled from the real rows
    (`design` = {cell: (n_base, n_cand)} overrides the two numbers per cell)."""
    rows = []
    for cell, _ in CELLS4:
        nb, nc = (design or {}).get(cell, (n_base, n_cand))
        for s in SEEDS:
            src = pools[cell][s]
            for a, n in (("BASE", nb // 2), ("CAND", nc // 2)):
                pick = [rng.choice(src) for _ in range(n)] if replace else rng.sample(src, n)
                rows += [row_for(r, a, f"{a}{i}") for i, r in enumerate(pick)]
    return rows


def key_of(cell, clause):
    return {"goodput": "mbps", "cpu_cli/GB": "cpu_cli", "cpu_srv/GB": "cpu_srv", "lag_E_cli": "cli_lag_E", "lag_E_srv": "srv_lag_E"}.get(clause, clause)


def shift(rows, cell, clause, delta):
    """CAND rows of (cell, clause) made worse by delta (relative): goodput x (1 - delta), everything else x (1 + delta)."""
    k = key_of(cell, clause)
    for r in rows:
        if r["cell"] == cell and r["arm"] == "CAND" and r.get(k) is not None:
            r[k] = r[k] * (1 - delta) if clause == "goodput" else r[k] * (1 + delta)


def separate(rows, cell, clause):
    """CAND rows fully separated on the worse side of every BASE row of (cell, clause), whatever the noise (10 % beyond the extreme)."""
    k = key_of(cell, clause)
    base = [r[k] for r in rows if r["cell"] == cell and r["arm"] == "BASE" and r.get(k) is not None]
    cand = [r for r in rows if r["cell"] == cell and r["arm"] == "CAND"]
    if clause == "goodput":
        edge = min(base) * 0.9
        for i, r in enumerate(cand):
            r[k] = edge - 0.01 * i * abs(edge)
    else:
        edge = max(base) * 1.1
        for i, r in enumerate(cand):
            r[k] = edge + 0.01 * i * abs(edge)


def p_of(method, rows, nperm, cells=CELLS4, exclude=()):
    if method == "relabel-max":
        return g.statistic(rows, "BASE", "CAND", n_perm=nperm, cells=cells)["p"]
    return g.statistic_rank(rows, "BASE", "CAND", n_perm=nperm, cells=cells, exclude=exclude)["p"]


# ── workers (top level for multiprocessing) ──
def w_a(args):
    method, cell, clause, draw, nperm = args
    pools = load_pools()
    rng = random.Random(1000 * draw + zlib.crc32(f'{cell}/{clause}'.encode()) % 997)
    rows = draw_dataset(rng, pools)
    separate(rows, cell, clause)
    return (method, cell, clause, draw, p_of(method, rows, nperm))


def w_b(args):
    method, draw, nperm = args
    pools = load_pools()
    rows = draw_dataset(random.Random(7000 + draw), pools)
    return (method, draw, p_of(method, rows, nperm))


def w_real(args):
    """Exact A/A on the real rows of c1s and c1d: 3 of the 9 rows of each seed are 'CAND', the other 6 are BASE (the guard's design)."""
    method, draw, nperm = args
    pools = load_pools()
    rng = random.Random(9000 + draw)
    rows = []
    for cell in ("c1s-400", "c1d-400"):
        for s in SEEDS:
            src = pools[cell][s]
            cand = set(rng.sample(range(len(src)), 3))
            for i, r in enumerate(src):
                rows.append(row_for(r, "CAND" if i in cand else "BASE", f"{s}-{i}"))
    return (method, draw, p_of(method, rows, nperm, cells=[("c1s-400", "def"), ("c1d-400", "def")]))


def w_c(args):
    method, cell, clause, delta, draw, nperm, n_cand = args
    pools = load_pools()
    rng = random.Random(31 * draw + int(delta * 1000) + zlib.crc32(f'{cell}/{clause}'.encode()) % 991)
    rows = draw_dataset(rng, pools, 12, n_cand)
    shift(rows, cell, clause, delta)
    return (method, cell, clause, delta, draw, p_of(method, rows, nperm))


def w_d(args):
    """The registered design: power / A/A / separated-clause at DESIGN, single-session rows, optionally without the lag clauses."""
    kind, variant, cell, clause, delta, draw, nperm = args
    pools = load_pools(single_session=True)
    ex = NOLAG if variant == "nolag" else ()
    rng = random.Random(77 * draw + int(delta * 1000) + zlib.crc32(f"{cell}/{clause}".encode()) % 983)
    rows = draw_dataset(rng, pools, design=DESIGN)
    if kind == "power":
        shift(rows, cell, clause, delta)
    elif kind == "sep":
        separate(rows, cell, clause)
    return (kind, variant, cell, clause, delta, draw, p_of("rank-minp", rows, nperm, exclude=ex))


def run(tasks, fn, procs=14):
    t0 = time.time()
    with mp.Pool(procs) as pool:
        out = pool.map(fn, tasks, chunksize=1)
    print(f"  [{len(tasks)} evaluations in {time.time() - t0:.0f} s]", file=sys.stderr)
    return out


def arg(argv, name, default):
    return type(default)(argv[argv.index(name) + 1]) if name in argv else default


def exp_a(argv):
    draws, nperm = arg(argv, "--draws", 10), arg(argv, "--nperm", 10000)
    tasks = [(m, c, cl, d, nperm) for m in ("relabel-max", "rank-minp") for c, _ in CELLS4 for cl in clause_list(c) for d in range(draws)]
    res = run(tasks, w_a)
    print(f"(a) ONE FULLY SEPARATED CLAUSE (CAND beyond every BASE row by >= 10 %), the other 25 clauses null; {draws} bootstrap datasets per clause, N_PERM={nperm}")
    print("    cell clause: p (max over draws) for relabel-max | rank-minp;  accept iff every p <= 0.01")
    worst = {"relabel-max": 0.0, "rank-minp": 0.0}
    for c, _ in CELLS4:
        for cl in clause_list(c):
            cell_res = {}
            for m in ("relabel-max", "rank-minp"):
                ps = [r[4] for r in res if r[0] == m and r[1] == c and r[2] == cl]
                cell_res[m] = (max(ps), statistics.mean(ps))
                worst[m] = max(worst[m], max(ps))
            print(f"    {c:<8} {cl:<14} relabel-max max {cell_res['relabel-max'][0]:.4f} mean {cell_res['relabel-max'][1]:.4f} | "
                  f"rank-minp max {cell_res['rank-minp'][0]:.4f} mean {cell_res['rank-minp'][1]:.4f}")
    for m in worst:
        print(f"    (a) {m}: worst p over all clauses and draws = {worst[m]:.4f} -> {'ACCEPT' if worst[m] <= 0.01 else 'REJECT'}")


def exp_b(argv):
    draws, nperm = arg(argv, "--draws", 400), arg(argv, "--nperm", 2000)
    tasks = [(m, d, nperm) for m in ("relabel-max", "rank-minp") for d in range(draws)]
    res = run(tasks, w_b)
    tasks = [(m, d, nperm) for m in ("relabel-max", "rank-minp") for d in range(draws)]
    real = run(tasks, w_real)
    print(f"(b) A/A: share of datasets with p <= 0.05 (the false-alarm rate; expect about 0.05), N_PERM={nperm}")
    for label, rr, cells in (("bootstrap of the real rows, four cells, BASE 12 vs 'CAND' 6", res, 4), ("real A/A splits of the 18 real BASE rows of c1s + c1d (3 + 3 of 9 + 9 vs the rest)", real, 2)):
        for m in ("relabel-max", "rank-minp"):
            ps = [r[2] for r in rr if r[0] == m]
            k = sum(1 for p in ps if p <= 0.05)
            lo = statistics.mean(ps)
            print(f"    {label}: {m}: {k}/{len(ps)} = {k / len(ps):.3f} (95 % binomial band about +-{1.96 * (0.05 * 0.95 / len(ps)) ** 0.5:.3f}); mean p {lo:.3f}")


def exp_c(argv):
    draws, nperm, n_cand = arg(argv, "--draws", 60), arg(argv, "--nperm", 2000), arg(argv, "--cand", 6)
    methods = ("relabel-max", "rank-minp")
    only_cells = arg(argv, "--cells", "").split(",") if "--cells" in argv else None
    only_clauses = arg(argv, "--clauses", "").split(",") if "--clauses" in argv else None
    cells_run = [(c, st) for c, st in CELLS4 if only_cells is None or c in only_cells]
    clauses_of = lambda c: [cl for cl in clause_list(c) if only_clauses is None or cl in only_clauses]  # noqa: E731
    tasks = [(m, c, cl, dl, d, nperm, n_cand) for m in methods for c, _ in cells_run for cl in clauses_of(c) for dl in GRID for d in range(draws)]
    res = run(tasks, w_c)
    print(f"(c) POWER from resampled BASE rows (CAND n = {n_cand}, BASE n = 12 per cell): share of {draws} draws with gate p <= 0.05 when ONE clause of ONE cell is made worse "
          f"by delta (goodput x (1 - delta), the rest x (1 + delta)); N_PERM={nperm}")
    print("    per clause: smallest grid delta with power >= 0.8 | power at the named shifts (client / server CPU +2.4 %, goodput -5 %)")
    for c, _ in cells_run:
        for cl in clauses_of(c):
            line = f"    {c:<8} {cl:<14}"
            for m in methods:
                pw = {}
                for dl in GRID:
                    ps = [r[5] for r in res if r[0] == m and r[1] == c and r[2] == cl and r[3] == dl]
                    pw[dl] = sum(1 for p in ps if p <= 0.05) / len(ps)
                ok = [dl for dl in GRID if pw[dl] >= 0.8]
                small = f"{100 * min(ok):.1f} %" if ok else ">100 %"
                nm = f" @{100 * NAMED[cl]:.1f}%={pw[NAMED[cl]]:.2f}" if cl in NAMED else ""
                line += f" | {m}: d80={small}{nm}"
            print(line)
    return res


def exp_rescore(argv):
    for label, d, cells, arm in (("41b", "place41b", g.CELLS, "MAIN"), ("41", "place41", g.CELLS, "MAIN")):
        rows, _ = pp.rows_of([os.path.join(RAW, d, "ld.log")])
        print(f"== re-score of the committed {label} ledger (BASE against {arm}; WIRE ignored)")
        for method in ("relabel-max", "rank-minp"):
            lines = []
            g.report(rows, out=lines.append, cells=cells, test_arm=arm, with_wire=False, method=method, dry=(label == "41"))
            for ln in lines:
                if ln.startswith("GATE-41b") or ln.lstrip().startswith(("min-p (true)", "T(true)", "p = ")):
                    print("    " + ln.strip())
            if method == "rank-minp":
                for ln in lines:
                    if ln.startswith("    c1") and "p=" in ln:
                        print("      " + ln.strip())


def exp_d(argv):
    """The registered design (status 47): CAND 12 / BASE 12 at c1s and c1d, CAND 6 / BASE 6 at c2 and c8, one interleaved session, balanced per seed;
    rows resampled from ONE session per cell. Reports (a), (b), (c) for rank-minp with all 26 clauses and with the two lag_E clauses reported-only."""
    draws, nperm = arg(argv, "--draws", 60), arg(argv, "--nperm", 2000)
    variants = ("all", "nolag")
    print(f"DESIGN {DESIGN} (n_base, n_cand per cell); rows: 41b BASE (c1s, c1d: 6 distinct per seed), 38 MAIN (c2, c8: 3 distinct per seed); resampled with replacement")
    # (a) separated clause, every clause
    tasks = [("sep", v, c, cl, 0.0, d, 10000) for v in variants for c, _ in CELLS4 for cl in clause_list(c) if not (v == "nolag" and cl in NOLAG) for d in range(5)]
    res = run(tasks, w_d)
    print(f"(a) one fully separated clause at one cell (5 datasets per clause, N_PERM=10000): worst p")
    for v in variants:
        ps = [r[6] for r in res if r[1] == v]
        print(f"    {v}: worst {max(ps):.4f}, mean {sum(ps) / len(ps):.4f} over {len(ps)} cases -> {'ACCEPT' if max(ps) <= 0.01 else 'REJECT'}")
    # (b) A/A
    nb = arg(argv, "--aa", 600)
    tasks = [("aa", v, "-", "-", 0.0, d, nperm) for v in variants for d in range(nb)]
    res = run(tasks, w_d)
    print(f"(b) A/A bootstrap at the design ({nb} datasets, N_PERM={nperm}): share with p <= 0.05")
    for v in variants:
        ps = [r[6] for r in res if r[1] == v]
        print(f"    {v}: {sum(1 for p in ps if p <= 0.05)}/{len(ps)} = {sum(1 for p in ps if p <= 0.05) / len(ps):.3f}; mean p {sum(ps) / len(ps):.3f}")
    # (c) power
    tasks = [("power", v, c, cl, dl, d, nperm) for v in variants for c, _ in CELLS4 for cl in clause_list(c) if not (v == "nolag" and cl in NOLAG)
             for dl in GRID for d in range(draws)]
    res = run(tasks, w_d)
    print(f"(c) POWER at the design ({draws} draws per point, N_PERM={nperm}): smallest grid delta with power >= 0.8, and the power at +2.4 % CPU / -5 % goodput; all 26 clauses | lag_E reported-only")
    for c, _ in CELLS4:
        for cl in clause_list(c):
            line = f"    {c:<8} {cl:<14}"
            for v in variants:
                if v == "nolag" and cl in NOLAG:
                    line += " | nolag: (reported only)"
                    continue
                pw = {}
                for dl in GRID:
                    ps = [r[6] for r in res if r[1] == v and r[2] == c and r[3] == cl and r[4] == dl]
                    pw[dl] = sum(1 for p in ps if p <= 0.05) / len(ps)
                ok = [dl for dl in GRID if pw[dl] >= 0.8]
                nm = f" @{100 * NAMED[cl]:.1f}%={pw[NAMED[cl]]:.2f}" if cl in NAMED else ""
                line += f" | {v}: d80={(str(round(100 * min(ok), 1)) + ' %') if ok else '>100 %'}{nm}"
            print(line)


def main(argv):
    if not argv:
        print(__doc__)
        return 2
    {"a": exp_a, "b": exp_b, "c": exp_c, "rescore": exp_rescore, "d": exp_d}[argv[0]](argv[1:])
    return 0


if __name__ == "__main__":
    mp.freeze_support()
    sys.exit(main(sys.argv[1:]))
