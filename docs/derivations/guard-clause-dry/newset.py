#!/usr/bin/env python3
"""The proposed clause set, dry-run (nothing in tools/l1 is changed; place41b_score.clause_defs is patched in this process only).
NEW SET (env NEWSET=1): rtp_floor_p<i> is dropped from scoring at the cells in NEWSET_DROP (default c2-100,c8-100); the one-sided clause
`inorder_delay_mean` (row key inorder_mean, ms, higher is worse) is added at the cells in NEWSET_ADD (default none). Every other clause is unchanged.
Imported first by the scripts below so that spawned pool workers get the same patch (they inherit the environment).
usage: python -I newset.py score <rowvals.json> <ld.log> <out.json> <cells> <add-cells|-> [ref:test]
       python -I newset.py d80   <rowvals.json> <ld.log> <out.json> <cell:nB:nC,...> <add-cells> [procs]"""
import json
import multiprocessing as mp
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "..", "..", "tools", "l1"))
import place38_parse as pp  # noqa: E402
import place41b_score as ps  # noqa: E402

_ORIG = ps.clause_defs
if os.environ.get("NEWSET") == "1":
    _DROP = set(filter(None, os.environ.get("NEWSET_DROP", "c2-100,c8-100").split(",")))
    _ADD = set(filter(None, os.environ.get("NEWSET_ADD", "").split(",")))

    def _new_defs(cell):
        d = [x for x in _ORIG(cell) if not (x[0].startswith("rtp_floor") and cell in _DROP)]
        if cell in _ADD:
            d.append(("inorder_delay_mean", "inorder_mean", 1))
        return d

    ps.clause_defs = _new_defs


def load_rows(rv, ld):
    rows, _ = pp.rows_of([ld])
    rows, _ = pp.apply_replacements(rows)
    for r in rows:
        v = rv.get("|".join([r["cell"], r["arm"], str(r["seed"]), str(r["rep"])]))
        r["inorder_mean"] = None if not v or not v.get("B") else v["B"]["mean"] * 1e3
        r["wall_s"] = r.get("wall_s")
    return rows


def do_score(rvp, ld, out, cells, add, pair):
    rows = load_rows(json.load(open(rvp)), ld)
    ref, test = pair.split(":")
    cl = [(c, "def") for c in cells.split(",")]
    st = ps.statistic_rank(rows, ref, test, cells=cl)
    res = {"p": st["p"], "T_true": st["T_true"], "argmin": list(st["argmin"]) if "argmin" in st else None, "pass": st["p"] > ps.P_PASS,
           "p_true": {"%s|%s" % k: v for k, v in st["p_true"].items()}}
    json.dump(res, open(out, "w"), indent=1)
    print(out, "p=%.4f min-p=%.5f at %s" % (st["p"], st["T_true"], res["argmin"]))


def do_d80(rvp, ld, out, spec, procs):
    rows = load_rows(json.load(open(rvp)), ld)
    cells = [(c.split(":")[0], "def") for c in spec.split(",")]
    design = {c.split(":")[0]: (int(c.split(":")[1]), int(c.split(":")[2])) for c in spec.split(",")}
    data = {}
    for cell, _ in cells:
        live = ps.live(rows, cell, "BASE")
        data[cell] = {sd: [r for r in live if r["seed"] == sd and not r.get("dnf")] for sd in sorted({r["seed"] for r in live})}
    clauses = [(c, n) for c, _ in cells for n, _, _ in ps.clause_defs(c) if n == "inorder_delay_mean" or n.startswith("rtp_floor")]
    tasks = [(c, n, d, k, tuple(cells), dict(design)) for c, n in clauses for d in ps.RES_GRID for k in range(ps.RES_DRAWS)]
    with mp.Pool(procs, initializer=ps._res_init, initargs=(data,)) as pool:
        hits = pool.map(ps._res_eval, tasks, chunksize=8)
    caught = {}
    for t, h in zip(tasks, hits):
        caught.setdefault((t[0], t[1], t[2]), []).append(h)
    res = {}
    for c, n in clauses:
        pw = {d: sum(caught[(c, n, d)]) / len(caught[(c, n, d)]) for d in ps.RES_GRID}
        ok = [d for d in ps.RES_GRID if pw[d] >= ps.RES_POWER]
        res["%s|%s" % (c, n)] = {"d80": min(ok) if ok else None, "power": pw}
    json.dump(res, open(out, "w"), indent=1)
    print(out, {k: v["d80"] for k, v in res.items()})


if __name__ == "__main__":
    a = sys.argv[1:]
    if a[0] == "score":
        do_score(a[1], a[2], a[3], a[4], a[5], a[6] if len(a) > 6 else "BASE:CAND")
    else:
        do_d80(a[1], a[2], a[3], a[4], int(a[6]) if len(a) > 6 else 4)
