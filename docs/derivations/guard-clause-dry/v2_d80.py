#!/usr/bin/env python3
"""The owed table of status 41d item 1: the A/A d80 of every c2 / c8 clause at 600 MB (S48b's rows) beside the guard-size d80 (S52 guard's rows), by the
guard's own recipe (place41b_score.session_resolution: BASE rows resampled with replacement to the 12 + 12 design, balanced per seed, CAND shifted worse by
delta, 30 draws, 1 000 relabellings, gate p <= 0.05 over ALL clauses of the four cells), on the production clause-set code.

Columns (the clause FAMILY differs and moves d80 on its own, so each is stated):
  guard    S52 guard ledger (c2 / c8 at guard size), family v1 (26 clauses): what `resolution41c` prints for this ledger
  guard22  the same rows, family without the floor (20 clauses): the row-size effect isolated from the family effect
  long     c1s / c1d BASE rows of the S52 guard ledger (guard size) + c2 / c8 BASE rows of the S48b ledger (600 MB, in-order means of rowvals-bbr48b.json),
           family v2 (22 clauses): the design v2 will run (the splice is the only one possible without a VM)
Only the clauses of c2 / c8 are evaluated (the statistic runs over all four cells). Seeds 0, 1, 2 (draw offsets 0, 100 000, 200 000).
usage: python -I v2_d80.py <out.json> [procs]   (from any directory; reads docs/l1-raw and rowvals-bbr48b.json)"""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "..", "..", "tools", "l1"))
import place38_parse as pp  # noqa: E402
import place41b_score as ps  # noqa: E402

RAW = os.path.join(HERE, "..", "..", "l1-raw")
LD52 = os.path.join(RAW, "s52", "guard-ld.log")
LD48 = os.path.join(RAW, "bbr48b", "run", "bbr48", "ld.log")


def load(ld):
    rows, _ = pp.rows_of([ld])
    rows, _ = pp.apply_replacements(rows)
    return rows


def with_inorder(rows):
    rv = json.load(open(os.path.join(HERE, "rowvals-bbr48b.json")))
    for r in rows:
        v = rv.get("|".join([r["cell"], r["arm"], str(r["seed"]), str(r["rep"])]))
        r["inorder_mean"] = None if not v or not v.get("B") else v["B"]["mean"] * 1e3
    return rows


def main(out, procs):
    ps.RES_MAX_PROCS = procs
    r52, r48 = load(LD52), with_inorder(load(LD48))
    long_rows = [r for r in r52 if r["cell"] in ("c1s-400", "c1d-400")] + [r for r in r48 if r["cell"] in ps.LONG_CELLS]
    cols = {"guard": (r52, "v1"), "guard22": (r52, {"floor": (), "inorder": ()}), "long": (long_rows, "v2")}
    res = {}
    for col, (rows, cs) in cols.items():
        for seed in (0, 1, 2):
            sr = ps.session_resolution(rows, only_cells=ps.LONG_CELLS, procs=procs, seed=seed, clause_set=cs)
            for (cell, clause), v in sr.items():
                res.setdefault("%s|%s|%s" % (col, cell, clause), {})[str(seed)] = {"d80": v["d80"], "power": {str(k): x for k, x in v["power"].items()}}
            print(col, "seed", seed, {"%s|%s" % k: v["d80"] for k, v in sr.items()}, flush=True)
    json.dump(res, open(out, "w"), indent=1)


if __name__ == "__main__":
    main(sys.argv[1], int(sys.argv[2]) if len(sys.argv) > 2 else 12)
