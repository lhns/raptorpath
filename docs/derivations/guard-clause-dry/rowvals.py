#!/usr/bin/env python3
"""Per-row values of the three candidate guard clauses from a ledger's ld.log and its extracted diag-ld directory.
usage: python -I rowvals.py <out.json> <ld.log> <diag-dir> [<ld.log> <diag-dir> ...]   (the directories are data)
Row key: cell|arm|seed|rep. C = rtp_floor (current), A = rtp_floor_source_phase, B = inorder_delay_stats (whole run; Bs = source-only)."""
import glob
import gzip
import json
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "..", "tools", "l1"))
import place38_parse as pp  # noqa: E402
import threadp1_parse as t1  # noqa: E402


def read(path):
    op = gzip.open if path.endswith(".gz") else open
    with op(path, "rt", errors="replace") as f:
        return f.read().splitlines()


def main(out, pairs):
    res = {}
    for ld, d in pairs:
        rows, _ = pp.rows_of([ld])
        rows, _info = pp.apply_replacements(rows)
        for r in rows:
            pat = os.path.join(glob.escape(d), "%s@%s-%s-s%s-r%s-a*-c.log*" % (r["cell"], r["store"], r["arm"], r["seed"], str(r["rep"]).lstrip("r")))
            fs = sorted(glob.glob(pat))
            key = "|".join([r["cell"], r["arm"], str(r["seed"]), str(r["rep"])])
            if not fs:
                res[key] = {"status": r["status"], "missing_log": True}
                continue
            cli = read(fs[-1])
            det = t1.rtp_floor_source_phase(cli, detail=True)
            best, first, last_src, last_t = {}, {}, 0.0, 0.0
            for t, src, legs in t1._rtp_ticks(cli):
                last_t = t
                if src > 0:
                    last_src = t
                for i, v in legs.items():
                    first.setdefault(i, v)
                    if i not in best or v < best[i][0]:
                        best[i] = (v, t)
            res[key] = {"status": r["status"], "bytes": r.get("bytes"),
                        "C": t1.rtp_floor(cli), "C_first": first, "C_t": {i: t for i, (v, t) in best.items()},
                        "A": {i: v[0] for i, v in det.items()}, "A_t": {i: v[1] for i, v in det.items()},
                        "A_first": {i: v[2] for i, v in det.items()},
                        "B": t1.inorder_delay_stats(cli), "Bs": t1.inorder_delay_stats(cli, True),
                        "last_src_t": last_src, "last_t": last_t}
    json.dump(res, open(out, "w"))
    print(len(res), "rows,", sum(1 for v in res.values() if v.get("missing_log")), "without a log")


if __name__ == "__main__":
    a = sys.argv[1:]
    main(a[0], list(zip(a[1::2], a[2::2])))
