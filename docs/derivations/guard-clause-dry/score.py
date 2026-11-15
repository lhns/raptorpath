#!/usr/bin/env python3
"""Dry-run scoring of the candidate guard clauses (A source-phase floor, B in-order delay mean / p95, C current rtp_floor) on a ledger.
usage: python -I score.py <rowvals.json> <ld.log> <out-prefix> <ref_arm>:<test_arm>[,<ref>:<test>...] <cell>[,<cell>...] [more arms to list]
Per (cell, candidate, leg): row values per arm, median, range, relative spread ((max-min)/median), ref->test median ratio, and the one-sided
rank-sum p of place41b_score.statistic_rank (N_PERM 10 000, RNG seed 4141: the guard's own machinery) for that clause. The p of a clause does not
depend on the other clauses, but the draw stream does depend on the cell list, so the cells are passed exactly as the ledger's guard ran them."""
import json
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "..", "tools", "l1"))
import l1common as lc  # noqa: E402
import place38_parse as pp  # noqa: E402
import place41b_score as ps  # noqa: E402

# candidate -> (label, unit factor, getter(rowvals, leg) -> value or None)
CANDS = {
    "C": ("C rtp_floor (current)", 1.0, lambda v, i: v["C"].get(str(i))),
    "A": ("A source-phase floor", 1.0, lambda v, i: v["A"].get(str(i))),
    "Bmean": ("B in-order delay mean", 1e3, lambda v, i: v["B"] and v["B"]["mean"]),
    "Bp95": ("B in-order delay p95", 1e3, lambda v, i: v["B"] and v["B"]["p95"]),
    "Bsmean": ("B' source-phase in-order delay mean", 1e3, lambda v, i: v["Bs"] and v["Bs"]["mean"]),
    "Bsp95": ("B' source-phase in-order delay p95", 1e3, lambda v, i: v["Bs"] and v["Bs"]["p95"]),
}
UNIT = {"C": "us", "A": "us", "Bmean": "ms", "Bp95": "ms", "Bsmean": "ms", "Bsp95": "ms"}


def rows_with(base_rows, rv, cand, cell_legs):
    """Copy the ledger rows with rtp_floor_p<i> replaced by the candidate's value (None where undefined). B is one scalar per row: leg 0 only."""
    out = []
    for r in base_rows:
        r = dict(r)
        v = rv.get("|".join([r["cell"], r["arm"], str(r["seed"]), str(r["rep"])]))
        for i in range(ps.sp.N_LEGS[r["cell"]]):
            k = "rtp_floor_p%d" % i
            if v is None or v.get("missing_log") or (cand.startswith("B") and i > 0):
                r[k] = None
            else:
                x = CANDS[cand][2](v, i)
                r[k] = None if x is None else x * CANDS[cand][1]
        out.append(r)
    return out


def main(rvp, ld, prefix, pairs, cells, extra):
    rv = json.load(open(rvp))
    rows, _ = pp.rows_of([ld])
    rows, info = pp.apply_replacements(rows)
    pairs = [tuple(p.split(":")) for p in pairs.split(",")]
    cells = [(c, "def") for c in cells.split(",")]
    res = {}
    tsv = []
    for cand in CANDS:
        rr = rows_with(rows, rv, cand, None)
        for ref, test in pairs:
            st = ps.statistic_rank(rr, ref, test, cells=cells)
            for cell, _ in cells:
                for i in range(ps.sp.N_LEGS[cell]):
                    if cand.startswith("B") and i > 0:
                        continue
                    arms = [ref, test] + [a for a in extra if a not in (ref, test)]
                    ent = {"p": st["p_true"].get((cell, "rtp_floor_p%d" % i)), "unit": UNIT[cand], "arms": {}}
                    for a in arms:
                        xs = sorted((r for r in rr if r["cell"] == cell and r["arm"] == a and r["status"] == "LIVE"), key=lambda r: (int(r["seed"]), int(str(r["rep"]).lstrip("r"))))
                        vals = [r["rtp_floor_p%d" % i] for r in xs]
                        d = [x for x in vals if x is not None]
                        for r, x in zip(xs, vals):
                            tsv.append("\t".join([cell, cand, "p%d" % i, a, str(r["seed"]), str(r["rep"]), "" if x is None else "%.1f" % x]))
                        m = lc.med(d) if d else None
                        ent["arms"][a] = {"n": len(vals), "n_none": len(vals) - len(d), "med": m, "min": min(d) if d else None, "max": max(d) if d else None,
                                          "spread": (max(d) - min(d)) / m if d and m else None}
                    mr, mt = ent["arms"][ref]["med"], ent["arms"][test]["med"]
                    ent["ratio"] = mt / mr if mr and mt else None
                    res["%s|%s|p%d|%s>%s" % (cell, cand, i, ref, test)] = ent
    json.dump(res, open(prefix + ".json", "w"), indent=1)
    open(prefix + ".tsv", "w").write("cell\tcand\tleg\tarm\tseed\trep\tvalue\n" + "\n".join(tsv) + "\n")
    print(prefix, len(res), "entries; replaced slots", len(info["replaced"]), "ignored", len(info["ignored"]))


if __name__ == "__main__":
    a = sys.argv[1:]
    main(a[0], a[1], a[2], a[3], a[4], a[5:])
