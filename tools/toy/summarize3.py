"""Stage-3 summaries (three-knobs.md 9.5, 9.6, 11).  Reads out3/*, writes nf_stage3.json (freeze) or markdown to stdout.

    python summarize3.py ident     # P1 and P2 identities (also the sender-trace identity for seeds 0-2, which reruns the sims)
    python summarize3.py freeze    # call-cell A/A -> nf_stage3.json (frozen once, before any decision cell is read)
    python summarize3.py read K|EM|O
    python summarize3.py tables
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

OUT = os.path.join(HERE, "out3")
NF_PATH = os.path.join(HERE, "nf_stage3.json")
NUDGE = (0.98, 0.99, 1.0, 1.01, 1.02)
INF = math.inf
EPS = {"I2": 0.10, "L10": 0.10}                     # stage 2's per-cell epsilon (the T bar for rbf-like readouts; not used here)
CELL_LOSS = {"E-I2": "I2", "E-L10": "L10", "M-I2": "I2", "M-L10": "L10", "K-I2": "I2", "K-L10": "L10", "K-L10b": "L10"}
P1_KEYS = ("n_src", "delivered", "never_delivered", "open_end", "v_mean", "v_p50", "v_p99", "v_p999", "e_mean", "e_p99",
           "hol_sum_nonhole", "n_holes", "stall_n", "stall_total", "stall_p99", "repair_per_delivered",
           "floors_per_delivered", "ooo_per_delivered", "tail_time", "t_last")
MS3 = {"v_mean", "v_p50", "v_p99", "v_p999", "e_mean", "e_p99", "w_mean", "w_p50", "w_p99", "tail_time", "stall_total", "stall_p99"}
SHARES = ("share_any", "share_T", "share_by_0p4", "share_by_2", "win_out_any", "win_out_by0p4", "win_out_by2")


def tag(x):
    return "inf" if x == INF else ("%g" % x).replace("+", "").replace(".", "p")


def gname(T, H):
    return "G-T%s-H%s" % (tag(T), tag(H))


_cache = {}


def load():
    if "d" not in _cache:
        d = {}
        for f in glob.glob(os.path.join(OUT, "*", "*.json")):
            b = os.path.basename(f)[:-5]
            name, seed = b.rsplit("-", 1)
            d[(name, int(seed))] = json.load(open(f))
        _cache["d"] = d
    return _cache["d"]


def R(cell, arm, seed):
    return load()[("%s.%s" % (cell, arm), seed)]


def seeds_of(cell, arm):
    n = "%s.%s" % (cell, arm)
    return sorted(s for (m, s) in load() if m == n)


def flat(r, k):
    """One metric of one run; times in ms (as stage 2).  Stalled finish counts at the run limit, never dropped."""
    if k == "finish":
        v = r.get("finish")
        return 80000.0 if v is None else v * 1e3
    if k == "tail_time" and r.get("tail_time") is None:
        return 80000.0
    v = r.get(k)
    if v is None:
        return float("nan")
    return v * 1e3 if k in MS3 or k == "finish" else float(v)


def vals(cell, arm, k, seeds=None):
    ss = seeds if seeds is not None else seeds_of(cell, arm)
    return np.asarray([flat(R(cell, arm, s), k) for s in ss])


def mean(cell, arm, k):
    v = vals(cell, arm, k)
    return float(np.nanmean(v)) if len(v) else float("nan")


# ---- bars -----------------------------------------------------------------------------------
def T_bar(k, cell):
    """T for readout k with C3 as the reference (9.5).  Times in ms.  Rules, from stage 2's table (summarize2.T_of): means
    max(5 % of the reference, 0.01 ms); percentiles and tails 1 ms; repair per delivered max(5 % of ref, 1/delivered); ooo per
    delivered max(10 % of ref, 1/delivered).  Shares: max(1/units, 0.05 (1 - share_C3))."""
    r0 = R(cell, "C3", 0)
    if k in ("v_mean", "w_mean", "e_mean"):
        return max(0.05 * mean(cell, "C3", k), 0.01)
    if k in ("v_p50", "v_p99", "v_p999", "w_p50", "w_p99", "e_p99", "tail_time", "stall_p99", "stall_total", "finish"):
        return 1.0
    if k == "repair_per_delivered":
        return max(0.05 * mean(cell, "C3", k), 1.0 / r0["delivered"])
    if k == "floors_per_delivered":
        return max(0.05 * mean(cell, "C3", k), 1.0 / r0["delivered"])
    if k == "ooo_per_delivered":
        return max(0.1 * mean(cell, "C3", k), 1.0 / r0["delivered"])
    if k in SHARES:
        return max(1.0 / r0["units"], 0.05 * (1.0 - mean(cell, "C3", "share_any" if k == "share_T" else k)))
    raise KeyError(k)


def nf_s3():
    return json.load(open(NF_PATH))


def NF(k, cell):
    """Noise floor: the frozen stage-3 A/A (call cells); E and M cells take stage 2's Part T floor where it has the readout and
    the call-cell floor otherwise (worker decision)."""
    nf2 = S2.nf_table()["tail"]
    nf3 = nf_s3()["metrics"]
    if cell.startswith("K-") or cell == "O-I2":
        return nf3.get(k, 0.0)
    return nf2.get(k, nf3.get(k, 0.0))


def bar(k, cell):
    return max(T_bar(k, cell), NF(k, cell))


def read(x, m):
    return S2.read(x, m)


def fmt(rd, nd=4):
    return S2.fmt(rd, nd)


def diff(cell, a, b, k, seeds=None):
    ss = seeds if seeds is not None else sorted(set(seeds_of(cell, a)) & set(seeds_of(cell, b)))
    return vals(cell, a, k, ss) - vals(cell, b, k, ss)


def kp(cell, names5, k):
    """K = 2 J1 - J2 and P = 2 D1 - D2 per seed over a five-point set (summarize2.kp_stats, on out3 names)."""
    pts = {h: vals(cell, n, k) for h, n in zip(NUDGE, names5)}
    J1 = pts[1.01] - pts[0.99]
    J2 = pts[1.02] - pts[0.98]
    D1 = pts[1.0] - 0.5 * (pts[0.99] + pts[1.01])
    D2 = pts[1.0] - 0.5 * (pts[0.98] + pts[1.02])
    return {"K": 2 * J1 - J2, "P": 2 * D1 - D2}


# ---- identities -------------------------------------------------------------------------------
ID_CELLS = ("E-I2", "E-L10", "M-I2", "M-L10", "K-I2", "K-L10")


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
    import batch3
    batch3.build()
    bad = 0
    n = 0
    print("### P1: S2 at T = inf and T = 1e6 (H = inf) against C3 (S1c + R1, rho = 1), per cell and seed, float-equal on %d keys\n" % len(P1_KEYS))
    for cell in ID_CELLS:
        for arm in (gname(INF, INF), "S2L"):
            ok = 0
            for s in range(12):
                d = same(R(cell, arm, s), R(cell, "C3", s), P1_KEYS)
                n += 1
                if d:
                    bad += 1
                    print("DIFF", cell, arm, s, d[:3])
                else:
                    ok += 1
            print("- %s %s: %d/12 seeds identical" % (cell, arm, ok))
    print("\n### P1 trace identity (sender trace, seeds 0-2; reruns with want_trace)\n")
    import toy2
    tbad = 0
    for cell in ID_CELLS:
        for s in range(3):
            lc, d = batch3.base(cell)
            tr = {}
            for nm, c in (("C3", batch3.cmp_cfg(cell, "C3", s)), ("S2inf", batch3.new_cfg(cell, s, INF, INF)),
                          ("S2e6", batch3.new_cfg(cell, s, 1e6, INF))):
                c.want_trace = True
                tr[nm] = toy2.run(c)["_trace"]
            ok = tr["C3"] == tr["S2inf"] == tr["S2e6"]
            tbad += 0 if ok else 1
        print("- %s: traces %s (seeds 0-2, %d entries)" % (cell, "identical" if not tbad else "DIFFER", len(tr["C3"])))
    print("\n### P2: the two ends of L6\n")
    p2bad = 0
    for cell in ID_CELLS:
        ok1 = 0
        ok2 = 0
        for s in range(12):
            a, b = R(cell, gname(0.4, INF), s), R(cell, "S2R1", s)
            keys = [k for k in a if k not in ("recv", "gauges", "wall_s")]
            d = same(a, b, keys)
            dg = same(a["gauges"], b["gauges"], list(a["gauges"]))
            if d or dg:
                p2bad += 1
                print("DIFF P2a", cell, s, (d + dg)[:3])
            else:
                ok1 += 1
            if all(R(cell, gname(T_, 0.0), s)["hol_sum_nonhole"] == 0.0 and R(cell, gname(T_, 0.0), s)["v_p50"] == 0.0 for T_ in (0.4, 2.0, INF)):
                ok2 += 1
            else:
                p2bad += 1
                print("DIFF P2b", cell, s)
        print("- %s: RH(inf) = R1 on every readout and gauge in %d/12 seeds; RH(0) has hol_sum_nonhole = 0 and v_p50 = 0 at T = 0.4, 2 and inf in %d/12" % (cell, ok1, ok2))
    print("\nP1 seed-level diffs: %d of %d; trace diffs: %d; P2 diffs: %d" % (bad, n, tbad, p2bad))
    return bad + tbad + p2bad


# ---- A/A freeze -------------------------------------------------------------------------------
AA_KEYS = ("share_any", "share_T", "share_by_0p4", "share_by_2", "v_mean", "v_p50", "v_p99", "w_mean", "w_p50", "w_p99",
           "ooo_per_delivered", "repair_per_delivered", "floors_per_delivered", "stall_total", "finish", "tail_time",
           "win_out_any", "win_out_by0p4", "win_out_by2")


def freeze(force=False):
    nf = dict(metrics={}, selfcheck={}, notes="95th percentile over 12 seeds of |A - A'| (S2 T = 0.4, H = 0.02, phase variants 0 and 1), largest over K-I2 and K-L10; ms for times")
    for k in AA_KEYS:
        nf["metrics"][k] = max(float(np.percentile(np.abs(vals(c, "AA-0", k) - vals(c, "AA-1", k)), 95)) for c in ("K-I2", "K-L10"))
    bad = []
    for c in ("K-I2", "K-L10"):
        for k in AA_KEYS:
            d = vals(c, "AA-0", k) - vals(c, "AA-1", k)
            rd = read(d, T_bar(k, c))
            nf["selfcheck"]["%s-%s" % (c, k)] = rd
            if rd["word"].startswith("shown"):
                bad.append((c, k, rd))
    nf["selfcheck_shown"] = [(c, k) for c, k, _ in bad]
    return nf, bad


# ---- main ---------------------------------------------------------------------------------------
def main():
    cmd = sys.argv[1]
    if cmd == "ident":
        sys.exit(1 if ident() else 0)
    elif cmd == "freeze":
        nf, bad = freeze()
        for c, k, rd in bad:
            print("A/A shown:", c, k, fmt(rd))
        json.dump(nf, open(NF_PATH, "w"), indent=1)
        n_eq = sum(1 for v in nf["selfcheck"].values() if v["word"] == "equivalent")
        n_un = sum(1 for v in nf["selfcheck"].values() if v["word"] == "undetermined")
        print("frozen", NF_PATH, "equivalent", n_eq, "undetermined", n_un, "shown", len(bad))
    else:
        import report_s3
        report_s3.main(cmd)


if __name__ == "__main__":
    main()
