"""Stage-3d summaries (three-knobs.md 17.9).  Reads out3d/*, writes nf_stage3d.json (freeze) or text to stdout.  Built on summarize3c.py
(same bars, same K/P statistic, same bootstrap reading), with the stage 3d paths and cell list.

    python summarize3d.py ident          # Q3d-1 on the stored runs (key identities)
    python summarize3d.py ident-trace    # Q3d-1 on the sender trace, seeds 0-2 (reruns the simulations)
    python summarize3d.py live           # every stored run finished
    python summarize3d.py freeze         # A/A -> nf_stage3d.json (frozen once, before any decision cell is read)
    python summarize3d.py read q1..q9    # one prediction (report_s3d.py)
"""
import json
import math
import os
import sys

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import summarize3c as S  # noqa: E402

S.OUT = os.path.join(HERE, "out3d")
S.NF_PATH = os.path.join(HERE, "nf_stage3d.json")
INF = math.inf
SHORT = ("E-I2", "E-L10", "M-I2", "M-L10", "K-I2", "K-L10", "K-L10b", "CL", "OV")
BLK = ("K-I2@b", "K-L10@b", "K-L10b@b", "CL@b", "OV@b")
LONG = ("CL-long", "OV-long", "CL-long@b", "OV-long@b")
XC = ("X15", "X20")
ALL_CELLS = SHORT + BLK + LONG + XC + ("P2",)
S.ALL_CELLS = ALL_CELLS
NO_NUDGE_CELLS = XC + ("P2",) + LONG
AA_KEYS = S.AA_KEYS + ("recovery_time",)
TAG = S.TAG
gname = S.gname


def ident_keys():
    return S.ID_KEYS


def ident():
    bad = n = 0
    print("### Q3d-1: S5 at T = inf (C3S) against C3, and S5 at T = 1e6 against C3S; float-equal on %d keys\n" % len(S.ID_KEYS))
    for cell in ALL_CELLS:
        row = []
        for arm, ref in (("C3S", "C3"), ("ID-1e6", "C3S")):
            seeds = sorted(set(S.seeds_of(cell, arm)) & set(S.seeds_of(cell, ref)))
            if not seeds:
                row.append("%s: not run" % arm)
                continue
            ok = 0
            for s in seeds:
                d = S.same(S.R(cell, arm, s), S.R(cell, ref, s), S.ID_KEYS)
                n += 1
                if d:
                    bad += 1
                    print("DIFF", cell, arm, s, d[:3])
                else:
                    ok += 1
            row.append("%s vs %s: %d/%d" % (arm, ref, ok, len(seeds)))
        print("- %s: %s" % (cell, "; ".join(row)))
    print("\nseed-level comparisons: %d, differing: %d" % (n, bad))
    return bad


def live():
    tot = unf = 0
    lines = []
    for (name, seed), r in sorted(S.load().items()):
        if r.get("finished") is None:
            continue
        tot += 1
        if not r["finished"]:
            unf += 1
            lines.append((name, seed))
    print("finished flag: %d runs carry it, %d did not finish" % (tot, unf))
    for x in lines[:80]:
        print("  UNFINISHED", x)
    return tot, unf


def ident_trace(seeds=(0, 1, 2)):
    import batch3d
    import toy2
    batch3d.build()
    bad = 0
    for cell in ALL_CELLS:
        res = []
        for s in seeds:
            tr = {}
            for nm in ("C3", "C3S", "ID-1e6"):
                key = "%s.%s" % (cell, nm)
                if key not in batch3d.JOBS:
                    continue
                c = batch3d.JOBS[key][1](s)
                c.want_trace = True
                tr[nm] = toy2.run(c)["_trace"]
            ok = len(tr) == 3 and tr["C3"] == tr["C3S"] == tr["ID-1e6"]
            bad += 0 if ok else 1
            res.append("identical" if ok else "DIFFER")
        print("- %s: traces %s (seeds %s)" % (cell, "/".join(res), list(seeds)), flush=True)
    return bad


def freeze():
    """A/A: AA-1 (clock-phase variant 1) against C3S (S5 at T = inf, variant 0) per cell and readout, as stage 3c did; in the four long cells C3S
    has 3 seeds only, so C3 (bit-equal to C3S by the identity of Q3d-1) is the variant-0 arm there."""
    nf = dict(cells={}, selfcheck={}, notes="p95 over 12 seeds of |A - A'|, A = C3S (S5 at T = inf, H = inf, phase variant 0; C3 in the long cells), A' = AA-1 (variant 1), per cell and readout; ms for times. NF_K = NF_P = this floor.")
    bad = []
    for cell in ALL_CELLS:
        ref = "C3S" if S.have(cell, "C3S") else "C3"
        if not (S.have(cell, "AA-1") and S.have(cell, ref)):
            continue
        nf["cells"][cell] = {}
        for k in AA_KEYS:
            d = S.diffv(cell, ref, "AA-1", k)
            if not len(d) or np.all(np.isnan(d)):
                continue
            fin = np.abs(d[np.isfinite(d)])
            nf["cells"][cell][k] = float(np.percentile(fin, 95)) if len(fin) else 0.0
            if np.any(np.isinf(d)):
                nf["cells"][cell][k] = float("inf")
            rd = S.read(d, S.T_bar(k, cell))
            nf["selfcheck"]["%s-%s" % (cell, k)] = rd
            if rd["word"].startswith("shown"):
                bad.append((cell, k, rd))
    nf["selfcheck_shown"] = [(c, k) for c, k, _ in bad]
    return nf, bad


def main():
    cmd = sys.argv[1]
    if cmd == "ident":
        sys.exit(1 if ident() else 0)
    elif cmd == "live":
        live()
    elif cmd == "ident-trace":
        sys.exit(1 if ident_trace() else 0)
    elif cmd == "freeze":
        nf, bad = freeze()
        for c, k, rd in bad:
            print("A/A shown:", c, k, S.fmt(rd))
        if os.path.exists(S.NF_PATH) and "--force" not in sys.argv:
            print("nf_stage3d.json exists; not overwriting (frozen).  --force rewrites it and must be reported as a deviation.")
            return
        json.dump(nf, open(S.NF_PATH, "w"), indent=1)
        n_eq = sum(1 for v in nf["selfcheck"].values() if v["word"] == "equivalent")
        n_un = sum(1 for v in nf["selfcheck"].values() if v["word"].startswith("undetermined"))
        print("frozen", S.NF_PATH, "cells", len(nf["cells"]), "equivalent", n_eq, "undetermined", n_un, "shown", len(bad))
    else:
        import report_s3d
        report_s3d.main(cmd)


if __name__ == "__main__":
    main()
