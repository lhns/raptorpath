"""Stage-2 summaries: bars (7.7), A/A noise floors, readings.  Reads out2/*, writes nf_stage2.json (freeze) or markdown to stdout.

    python summarize2.py freeze     # Part A -> nf_stage2.json (frozen once; never recomputed by T and B readings)
    python summarize2.py A | T | B  # markdown tables
"""
import glob
import json
import math
import os
import sys

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "out2")
NF_PATH = os.path.join(HERE, "nf_stage2.json")
EPS = {"I1": 0.025, "I2": 0.10, "G": 0.025, "L10": 0.10}
PRESETS = {"Bulk": 0.005, "Auto": 0.5, "Realtime": 50.0}
RHO = {"Bulk": 1.0, "Auto": 1.0, "Realtime": 0.99}
NUDGE = (0.98, 0.99, 1.0, 1.01, 1.02)
OMEGAS = (0.0, 0.3, 1.0, 3.0, 10.0, 100.0)
KP_READ = ("e_mean", "e_p99", "rbf", "G_ratio", "repair_per_delivered")
LAT = ("v_p50", "v_p99", "v_p999", "e_p50", "e_p99", "e_p999", "tail_time", "tail_v_mean", "tail_v_p50")
MEANS = ("v_mean", "e_mean")
ALL_M = ("v_p50", "v_p99", "v_p999", "v_mean", "e_p50", "e_p99", "e_p999", "e_mean", "G_ratio", "max_ep_spend",
         "repair_per_delivered", "rbf", "fp_rel_pd", "ooo_per_delivered")
T_M = ALL_M + ("tail_time", "tail_v_mean", "tail_v_p50")
MS = {"v_p50", "v_p99", "v_p999", "v_mean", "e_p50", "e_p99", "e_p999", "e_mean", "tail_time", "tail_v_mean", "tail_v_p50"}


def tag(x):
    return ("%g" % x).replace("+", "")


_cache = {}


def load(part=None):
    if "all" not in _cache:
        d = {}
        for f in glob.glob(os.path.join(OUT, "*", "*.json")):
            b = os.path.basename(f)[:-5]
            name, seed = b.rsplit("-", 1)
            d[(name, int(seed))] = json.load(open(f))
        _cache["all"] = d
    return _cache["all"]


def flat(r, k):
    """One metric of one run, in the unit of the bars (ms for times)."""
    if k == "fp_rel_pd":
        return r["fp_release_filled"] / max(r["delivered"], 1)
    v = r.get(k)
    if k == "tail_time" and v is None:
        return 40000.0                      # stalled: counted at the 40 s limit, never dropped
    if v is None:
        return float("nan")
    return v * 1e3 if k in MS else float(v)


def seeds_of(name):
    return sorted(s for (n, s) in load() if n == name)


def vals(name, k, seeds=None):
    d = load()
    ss = seeds if seeds is not None else seeds_of(name)
    return np.asarray([flat(d[(name, s)], k) for s in ss])


def boot(x, n=4000, seed=1):
    x = np.asarray(x, float)
    x = x[~np.isnan(x)]
    if len(x) == 0:
        return float("nan"), float("nan"), float("nan")
    if len(x) < 2 or np.all(x == x[0]):
        return float(x.mean()), float(x.mean()), float(x.mean())
    rng = np.random.default_rng(seed)
    m = x[rng.integers(0, len(x), (n, len(x)))].mean(1)
    return float(x.mean()), float(np.percentile(m, 2.5)), float(np.percentile(m, 97.5))


def read(x, m):
    """7.7: shown larger if lo > m; shown smaller if hi < -m; equivalent if -m <= lo and hi <= m; else undetermined."""
    mu, lo, hi = boot(x)
    if math.isnan(mu):
        return dict(mean=mu, lo=lo, hi=hi, m=m, word="n/a")
    if lo > m:
        w = "shown larger"
    elif hi < -m:
        w = "shown smaller"
    elif lo >= -m and hi <= m:
        w = "equivalent"
    else:
        w = "undetermined"
    return dict(mean=mu, lo=lo, hi=hi, m=m, word=w)


def fmt(rd, nd=4):
    return "%.*g [%.*g, %.*g] vs m=%.3g: %s" % (nd, rd["mean"], nd, rd["lo"], nd, rd["hi"], rd["m"], rd["word"])


# ---- the bars (T) ---------------------------------------------------------------------------
def s0_mean(refname, k):
    v = vals(refname, k)
    return float(np.nanmean(v)) if len(v) else float("nan")


def T_of(k, cell, rho, n_src, delivered, ref):
    """T for metric k (7.7 table).  Relative T is against S0's value in the same cell (ref = the REF config's name)."""
    if k in MEANS:
        return max(0.05 * s0_mean(ref, k), 0.01)
    if k in ("v_p50", "v_p99", "v_p999", "e_p50", "e_p99", "e_p999", "tail_time", "tail_v_mean", "tail_v_p50"):
        return 1.0
    if k == "G_ratio":
        return max(0.05 * (1 - rho), 1.0 / n_src)
    if k == "max_ep_spend":
        return max(0.05 * math.floor((1 - rho) * n_src + 1e-9), 1.0)
    if k == "repair_per_delivered":
        return max(0.05 * s0_mean(ref, k), 1.0 / delivered)
    if k in ("rbf", "fp_rel_pd"):
        return EPS[cell] ** 2
    if k == "ooo_per_delivered":
        return max(0.1 * s0_mean(ref, k), 1.0 / delivered)
    raise KeyError(k)


def nf_table():
    return json.load(open(NF_PATH))


# ---- Part A: the freeze ---------------------------------------------------------------------
def p95abs(a, b):
    return float(np.percentile(np.abs(np.asarray(a) - np.asarray(b)), 95))


def kp_stats(names5, k, seeds):
    """Per seed K and P of readout k over the five points (7.9)."""
    m = {h: vals(n, k, seeds) for h, n in zip(NUDGE, names5)}

    def J(h):
        return m[1 + h] - m[1 - h] if False else None
    out = {}
    pts = {h: m[h] for h in NUDGE}
    J1 = pts[1.01] - pts[0.99]
    J2 = pts[1.02] - pts[0.98]
    D1 = pts[1.0] - 0.5 * (pts[0.99] + pts[1.01])
    D2 = pts[1.0] - 0.5 * (pts[0.98] + pts[1.02])
    out["K"] = 2 * J1 - J2
    out["P"] = 2 * D1 - D2
    return out


def freeze():
    d = load()
    nf = dict(metrics={}, tail={}, kp={}, selfcheck={}, notes="95th percentile over seeds of |A - A'| (arms differ only in clock phases); metrics in ms where times")
    cells = [("I1", "RT"), ("I1", "AU"), ("I2", "RT"), ("I2", "AU"), ("G", "RT"), ("G", "AU")]
    for k in ALL_M:
        nf["metrics"][k] = max(p95abs(vals("AA-%s-%s-A" % c, k), vals("AA-%s-%s-B" % c, k)) for c in cells)
    for k in T_M:
        nf["tail"][k] = max(p95abs(vals("AAT-I2-%s-A" % r, k), vals("AAT-I2-%s-B" % r, k)) for r in ("R0e", "R1"))
    for pre in ("Auto", "Realtime"):
        nf["kp"][pre] = {}
        for k in KP_READ:
            res = {}
            for stat in ("K", "P"):
                per = {}
                for arm in ("A", "B"):
                    names = ["KP-%s-%s-%s" % (pre, tag(h), arm) for h in NUDGE]
                    per[arm] = kp_stats(names, k, list(range(12)))[stat]
                res["NF_" + stat] = p95abs(per["A"], per["B"])
            nf["kp"][pre][k] = res
    nf["kp"]["Bulk"] = {k: {s: max(nf["kp"]["Auto"][k][s], nf["kp"]["Realtime"][k][s]) for s in ("NF_K", "NF_P")} for k in KP_READ}
    # A/A self-check: the mean A/A difference must read equivalent against T (not against max(T, NF))
    sc = {}
    bad = []
    for cell, pt in cells:
        rho = RHO["Realtime"] if pt == "RT" else 1.0
        a, b = "AA-%s-%s-A" % (cell, pt), "AA-%s-%s-B" % (cell, pt)
        n_src = d[(a, 0)]["n_src"]
        dlv = d[(a, 0)]["delivered"]
        for k in ALL_M:
            T = T_of(k, cell, rho, n_src, dlv, "REF-%s" % cell)
            rd = read(vals(a, k) - vals(b, k), T)
            sc["%s-%s-%s" % (cell, pt, k)] = rd
            if rd["word"] == "shown larger" or rd["word"] == "shown smaller":
                bad.append(("%s-%s" % (cell, pt), k, rd))
    for recv in ("R0e", "R1"):
        a, b = "AAT-I2-%s-A" % recv, "AAT-I2-%s-B" % recv
        n_src = d[(a, 0)]["n_src"]
        dlv = d[(a, 0)]["delivered"]
        for k in T_M:
            T = T_of(k, "I2", 0.99, n_src, dlv, "REFT-I2") if ("REFT-I2", 0) in d else 1.0
            rd = read(vals(a, k) - vals(b, k), T)
            sc["T-%s-%s" % (recv, k)] = rd
            if rd["word"] in ("shown larger", "shown smaller"):
                bad.append(("T-%s" % recv, k, rd))
    nf["selfcheck"] = sc
    nf["selfcheck_shown"] = [(c, k) for c, k, _ in bad]
    return nf, bad


# ---- tables -----------------------------------------------------------------------------------
def md_selfcheck(nf):
    n_eq = sum(1 for v in nf["selfcheck"].values() if v["word"] == "equivalent")
    n_un = [(k, v) for k, v in nf["selfcheck"].items() if v["word"] == "undetermined"]
    n_sh = [(k, v) for k, v in nf["selfcheck"].items() if v["word"].startswith("shown")]
    return n_eq, n_un, n_sh


def main():
    cmd = sys.argv[1]
    if cmd == "freeze":
        nf, bad = freeze()
        print("self-check shown:", [(c, k) for c, k, _ in bad])
        if bad and "--force" not in sys.argv:
            for c, k, rd in bad:
                print(c, k, fmt(rd))
            print("A/A SELF-CHECK FAILED: not writing the frozen table")
            json.dump(nf, open(NF_PATH + ".failed", "w"), indent=1)
            return
        json.dump(nf, open(NF_PATH, "w"), indent=1)
        n_eq, n_un, n_sh = md_selfcheck(nf)
        print("frozen:", NF_PATH, "equivalent", n_eq, "undetermined", len(n_un), "shown", len(n_sh))
    else:
        import report2
        report2.main(cmd)


if __name__ == "__main__":
    main()
