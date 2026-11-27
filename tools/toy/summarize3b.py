"""Stage-3b summaries (three-knobs.md 13.7.4, 13.7.5).  Reads out3b/*, writes nf_stage3b.json (freeze) or text to stdout.

    python summarize3b.py ident          # Q3b-1 on the stored runs (key identities)
    python summarize3b.py ident-trace    # Q3b-1 on the sender trace, seeds 0-2 (reruns the simulations)
    python summarize3b.py freeze         # A/A -> nf_stage3b.json (frozen once, before any decision cell is read)
    python summarize3b.py read Q3b-N     # one prediction (report_s3b.py)
"""
import glob
import json
import math
import os
import sys

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import summarize2 as S2  # noqa: E402

OUT = os.path.join(HERE, "out3b")
NF_PATH = os.path.join(HERE, "nf_stage3b.json")
NUDGE = (0.98, 0.99, 1.0, 1.01, 1.02)
INF = math.inf
P1_KEYS = ("n_src", "delivered", "never_delivered", "open_end", "v_mean", "v_p50", "v_p99", "v_p999", "e_mean", "e_p99",
           "hol_sum_nonhole", "n_holes", "stall_n", "stall_total", "stall_p99", "repair_per_delivered",
           "floors_per_delivered", "ooo_per_delivered", "tail_time", "t_last")
MS = {"v_mean", "v_p50", "v_p99", "v_p999", "e_mean", "e_p99", "w_mean", "w_p50", "w_p99", "tail_time", "stall_total", "stall_p99",
      "dh_p50", "dh_p90", "dh_p99", "finish", "R_mean", "srtt_mean", "resume_delay", "stall_max"}
SHARES = ("share_any", "share_T", "share_by_0p4", "share_by_2")
DH = ("dh_p50", "dh_p90", "dh_p99")
INF_KEYS = set(DH)
ALL_CELLS = ("E-I2", "E-I2n", "E-L10", "M-I2", "M-L10", "K-I2", "K-L10", "K-L10b", "CL", "OV", "X15", "X20")
AA_KEYS = SHARES + DH + ("v_mean", "v_p50", "v_p99", "w_mean", "ooo_per_delivered", "repair_per_delivered", "floors_per_delivered",
                         "stall_total", "tail_time", "finish", "pause_time", "probes", "R_mean", "link_util_w", "vta", "queue_drop",
                         "intake_expired_deq", "verdicts", "resume_delay")
TAG = lambda x: "inf" if x == INF else ("%g" % x).replace("+", "").replace(".", "p")          # noqa: E731
gname = lambda T, H: "G-T%s-H%s" % (TAG(T), TAG(H))                                          # noqa: E731

_cache = {}


def load():
    if "d" not in _cache:
        d = {}
        for f in glob.glob(os.path.join(OUT, "*", "*.json")):
            b = os.path.basename(f)[:-5]
            name, seed = b.rsplit("-", 1)
            with open(f) as fh:
                d[(name, int(seed))] = json.load(fh)
        _cache["d"] = d
    return _cache["d"]


def reload():
    _cache.clear()


def R(cell, arm, seed):
    return load()[("%s.%s" % (cell, arm), seed)]


def seeds_of(cell, arm):
    n = "%s.%s" % (cell, arm)
    return sorted(s for (m, s) in load() if m == n)


def have(cell, arm, n=12):
    return len(seeds_of(cell, arm)) >= n


def flat(r, k):
    """One metric of one run; times in ms.  A run that never finishes counts at the run limit (finish, tail_time)."""
    if k == "finish":
        v = r.get("finish")
        return 80000.0 if v is None else v * 1e3
    if k == "tail_time" and r.get("tail_time") is None:
        return 80000.0
    v = r.get(k)
    if v is None:
        return float("nan")
    return v * 1e3 if k in MS else float(v)


def vals(cell, arm, k, seeds=None):
    ss = seeds if seeds is not None else seeds_of(cell, arm)
    return np.asarray([flat(R(cell, arm, s), k) for s in ss], dtype=float)


def mean(cell, arm, k):
    v = vals(cell, arm, k)
    return float(np.nanmean(v)) if len(v) else float("nan")


# ---- bars -------------------------------------------------------------------------------------
def T_bar(k, cell):
    """T with the cell's own C3 as reference (13.7.4): shares max(1/units, 0.05 (1 - share_C3)); delay quantiles 1 ms; the rest as stage 3."""
    r0 = R(cell, "C3", 0)
    if k in ("v_mean", "w_mean", "e_mean"):
        return max(0.05 * mean(cell, "C3", k), 0.01)
    if k in ("v_p50", "v_p99", "v_p999", "w_p50", "w_p99", "e_p99", "tail_time", "stall_p99", "stall_total", "finish") or k in DH:
        return 1.0
    if k in ("repair_per_delivered", "floors_per_delivered"):
        return max(0.05 * mean(cell, "C3", k), 1.0 / max(r0["delivered"], 1))
    if k == "ooo_per_delivered":
        return max(0.1 * mean(cell, "C3", k), 1.0 / max(r0["delivered"], 1))
    if k in SHARES:
        return max(1.0 / r0["units"], 0.05 * (1.0 - mean(cell, "C3", "share_any" if k == "share_T" else k)))
    if k in ("pause_time", "R_mean", "srtt_mean", "resume_delay", "stall_max"):
        return 1.0 if k != "pause_time" else 1.0
    return max(1.0, 0.05 * abs(mean(cell, "C3", k)))                 # counts: probes, verdicts, queue_drop, intake_expired_deq, vta


def nf_all():
    if "nf" not in _cache:
        _cache["nf"] = json.load(open(NF_PATH)) if os.path.exists(NF_PATH) else None
    return _cache["nf"]


def NF(k, cell):
    nf = nf_all()
    if nf is None:
        return 0.0
    return nf["cells"].get(cell, {}).get(k, 0.0)


def bar(k, cell):
    return max(T_bar(k, cell), NF(k, cell))


def read(x, m):
    """summarize2.read, with +-inf handled: where both arms of a pair are infinite the difference is set to 0 by diffv, a one-sided
    infinity is a shown difference (bootstrap of inf).  Returns the stage 2 dict."""
    x = np.asarray(x, float)
    if len(x) and np.all(np.isinf(x)) and np.all(np.sign(x) == np.sign(x[0])):
        w = "shown larger" if x[0] > 0 else "shown smaller"
        return dict(mean=float(x[0]), lo=float(x[0]), hi=float(x[0]), m=m, word=w)
    if np.any(np.isinf(x)):                                            # mixed: finite and infinite seeds
        mu = float(np.mean(x))
        fin = x[np.isfinite(x)]
        pos, neg = int(np.sum(x == INF)), int(np.sum(x == -INF))
        w = "shown larger (some seeds infinite)" if pos > neg and (len(fin) == 0 or pos >= len(x) // 2) else "undetermined (infinite seeds)"
        return dict(mean=mu, lo=float("nan"), hi=float("nan"), m=m, word=w)
    return S2.read(x, m)


def fmt(rd, nd=4):
    return S2.fmt(rd, nd) if not (math.isinf(rd["mean"]) or math.isnan(rd["lo"])) else "%s [inf-seeds] vs m=%.3g: %s" % (rd["mean"], rd["m"], rd["word"])


def diffv(cell, a, b, k, seeds=None):
    ss = seeds if seeds is not None else sorted(set(seeds_of(cell, a)) & set(seeds_of(cell, b)))
    x, y = vals(cell, a, k, ss), vals(cell, b, k, ss)
    with np.errstate(invalid="ignore"):
        d = x - y
    d[np.isinf(x) & np.isinf(y) & (np.sign(x) == np.sign(y))] = 0.0       # both infinite: no difference
    return d


def kp(cell, names5, k):
    pts = {h: vals(cell, n, k) for h, n in zip(NUDGE, names5)}
    J1 = pts[1.01] - pts[0.99]
    J2 = pts[1.02] - pts[0.98]
    D1 = pts[1.0] - 0.5 * (pts[0.99] + pts[1.01])
    D2 = pts[1.0] - 0.5 * (pts[0.98] + pts[1.02])
    return {"K": 2 * J1 - J2, "P": 2 * D1 - D2, "J": J2}


# ---- identities (Q3b-1) -----------------------------------------------------------------------
def same(a, b, keys):
    out = []
    for k in keys:
        x, y = a.get(k), b.get(k)
        if x is None and y is None:
            continue
        if isinstance(x, float) and isinstance(y, float) and math.isnan(x) and math.isnan(y):
            continue
        if x != y:
            out.append((k, x, y))
    return out


def ident():
    bad = n = 0
    print("### Q3b-1: S3 (P) at T = inf and T = 1e6 against C3; S3 (R) at T = 1e6 against C3R; float-equal on %d keys, 12 seeds\n" % len(P1_KEYS))
    for cell in ALL_CELLS:
        row = []
        for arm, ref in (("ID-P-inf", "C3"), ("ID-P-1e6", "C3"), ("ID-R-1e6", "C3R")):
            if not (have(cell, arm) and have(cell, ref)):
                row.append("%s: not run" % arm)
                continue
            ok = 0
            for s in range(12):
                d = same(R(cell, arm, s), R(cell, ref, s), P1_KEYS)
                n += 1
                if d:
                    bad += 1
                    print("DIFF", cell, arm, s, d[:3])
                else:
                    ok += 1
            row.append("%s vs %s: %d/12" % (arm, ref, ok))
        print("- %s: %s" % (cell, "; ".join(row)))
    print("\nseed-level diffs: %d of %d" % (bad, n))
    return bad


def ident_trace(cells=ALL_CELLS, seeds=(0, 1, 2)):
    import batch3b
    import toy2
    batch3b.build()
    bad = 0
    for cell in cells:
        res = []
        for s in seeds:
            tr = {}
            for nm, c in (("C3", batch3b.cmp_cfg(cell, "C3", s)), ("P-inf", batch3b.s3_cfg(cell, s, INF, INF, tail="P")),
                          ("P-1e6", batch3b.s3_cfg(cell, s, 1e6, INF, tail="P")), ("C3R", batch3b.s3_cfg(cell, s, INF, INF)),
                          ("R-1e6", batch3b.s3_cfg(cell, s, 1e6, INF))):
                c.want_trace = True
                tr[nm] = toy2.run(c)["_trace"]
            ok = tr["C3"] == tr["P-inf"] == tr["P-1e6"] and tr["C3R"] == tr["R-1e6"]
            bad += 0 if ok else 1
            res.append("identical" if ok else "DIFFER")
        print("- %s: traces %s (seeds %s; %d entries C3, %d entries C3R)" % (cell, "/".join(res), list(seeds), len(tr["C3"]), len(tr["C3R"])), flush=True)
    return bad


# ---- A/A freeze -------------------------------------------------------------------------------
def freeze():
    nf = dict(cells={}, selfcheck={}, notes="p95 over 12 seeds of |A - A'|, S3 (R) at T = inf, H = inf, phase variants 0 (C3R) and 1 (AA-1), per cell and readout; ms for times. NF_K = NF_P = this floor.")
    bad = []
    for cell in ALL_CELLS:
        if not (have(cell, "C3R") and have(cell, "AA-1") and have(cell, "C3")):
            continue
        nf["cells"][cell] = {}
        for k in AA_KEYS:
            d = diffv(cell, "C3R", "AA-1", k)
            fin = np.abs(d[np.isfinite(d)])
            nf["cells"][cell][k] = float(np.percentile(fin, 95)) if len(fin) else 0.0
            if np.any(np.isinf(d)):
                nf["cells"][cell][k] = float("inf")
            rd = read(d, T_bar(k, cell))
            nf["selfcheck"]["%s-%s" % (cell, k)] = rd
            if rd["word"].startswith("shown"):
                bad.append((cell, k, rd))
    nf["selfcheck_shown"] = [(c, k) for c, k, _ in bad]
    return nf, bad


def main():
    cmd = sys.argv[1]
    if cmd == "ident":
        sys.exit(1 if ident() else 0)
    elif cmd == "ident-trace":
        sys.exit(1 if ident_trace() else 0)
    elif cmd == "freeze":
        nf, bad = freeze()
        for c, k, rd in bad:
            print("A/A shown:", c, k, fmt(rd))
        if os.path.exists(NF_PATH) and "--force" not in sys.argv:
            print("nf_stage3b.json exists; not overwriting (frozen).  --force rewrites it and must be reported as a deviation.")
            return
        json.dump(nf, open(NF_PATH, "w"), indent=1)
        n_eq = sum(1 for v in nf["selfcheck"].values() if v["word"] == "equivalent")
        n_un = sum(1 for v in nf["selfcheck"].values() if v["word"].startswith("undetermined"))
        print("frozen", NF_PATH, "cells", len(nf["cells"]), "equivalent", n_eq, "undetermined", n_un, "shown", len(bad))
    else:
        import report_s3b
        report_s3b.main(cmd)


if __name__ == "__main__":
    main()
