#!/usr/bin/env python3
"""A/A resolution (d80) of the candidate rtp clauses, by the guard's own session_resolution recipe (place41b_score: BASE rows resampled with
replacement to the design, balanced per seed, CAND rows shifted worse by delta, RES_DRAWS draws, RES_NPERM relabellings, gate p <= 0.05 over ALL
clauses of the listed cells), restricted to the rtp clauses (the other clauses keep their real BASE values).
usage: python -I resolution.py <rowvals.json> <ld.log> <out.json> <cell>:<nBASE>:<nCAND>[,...] [procs]"""
import json
import multiprocessing as mp
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "..", "tools", "l1"))
import place38_parse as pp  # noqa: E402
import place41b_score as ps  # noqa: E402
from score import CANDS, rows_with  # noqa: E402


def main(rvp, ld, out, spec, procs, only=None):
    rv = json.load(open(rvp))
    rows, _ = pp.rows_of([ld])
    rows, _ = pp.apply_replacements(rows)
    cells = [(c.split(":")[0], "def") for c in spec.split(",")]
    design = {c.split(":")[0]: (int(c.split(":")[1]), int(c.split(":")[2])) for c in spec.split(",")}
    res = {}
    for cand in (only or CANDS):
        rr = rows_with(rows, rv, cand, None)
        data = {}
        for cell, _ in cells:
            data[cell] = {sd: [r for r in ps.live(rr, cell, "BASE") if r["seed"] == sd and not r.get("dnf")] for sd in sorted({r["seed"] for r in ps.live(rr, cell, "BASE")})}
        tasks = []
        for cell, _ in cells:
            for i in range(ps.sp.N_LEGS[cell]):
                if cand.startswith("B") and i > 0:
                    continue
                for d in ps.RES_GRID:
                    for k in range(ps.RES_DRAWS):
                        tasks.append((cell, "rtp_floor_p%d" % i, d, k, tuple(cells), dict(design)))
        with mp.Pool(procs, initializer=ps._res_init, initargs=(data,)) as pool:
            hits = pool.map(ps._res_eval, tasks, chunksize=8)
        caught = {}
        for t, h in zip(tasks, hits):
            caught.setdefault((t[0], t[1], t[2]), []).append(h)
        for cell, _ in cells:
            for i in range(ps.sp.N_LEGS[cell]):
                if cand.startswith("B") and i > 0:
                    continue
                n = "rtp_floor_p%d" % i
                pw = {d: sum(caught[(cell, n, d)]) / len(caught[(cell, n, d)]) for d in ps.RES_GRID}
                ok = [d for d in ps.RES_GRID if pw[d] >= ps.RES_POWER]
                res["%s|%s|p%d" % (cell, cand, i)] = {"d80": min(ok) if ok else None, "power": pw}
        print(cand, "done", flush=True)
    json.dump(res, open(out, "w"), indent=1)


if __name__ == "__main__":
    a = sys.argv[1:]
    main(a[0], a[1], a[2], a[3], int(a[4]) if len(a) > 4 else 4, a[5].split(",") if len(a) > 5 else None)
