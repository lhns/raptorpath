"""Summary table + P5 readings from tools/toy/out/*.json -> results-stage1.md (stdout)."""
import glob
import json
import os
import sys

import numpy as np

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "out")
CANDS = ["S0", "S1", "S1c", "S2", "S3"]


def load():
    d = {}
    for f in glob.glob(os.path.join(OUT, "*.json")):
        b = os.path.basename(f)[:-5]
        cfg, cand, seed = b.rsplit("-", 2)
        d[(cfg, cand, int(seed))] = json.load(open(f))
    return d


def boot(x, n=4000, seed=1):
    x = np.asarray(x, float)
    if len(x) < 2:
        return float(x.mean()) if len(x) else float("nan"), float("nan"), float("nan")
    rng = np.random.default_rng(seed)
    m = x[rng.integers(0, len(x), (n, len(x)))].mean(1)
    return float(x.mean()), float(np.percentile(m, 2.5)), float(np.percentile(m, 97.5))


def main():
    d = load()
    cfgs = sorted({k[0] for k in d}, key=lambda c: list(CONFIG_ORDER).index(c) if c in CONFIG_ORDER else 99)
    seeds = lambda c, cand: sorted(s for (cc, ca, s) in d if cc == c and ca == cand)
    p = print
    p("| config | cand | seeds | G/S (1-rho=0.01) | HOL p50 ms | HOL p99 ms | stall p99 ms | repair/deliv | max-episode spend | gap | refusals | D-fail | P1 viol |")
    p("|---|---|---|---|---|---|---|---|---|---|---|---|---|")
    for c in cfgs:
        for cand in CANDS:
            ss = seeds(c, cand)
            if not ss:
                continue
            R = [d[(c, cand, s)] for s in ss]
            m = lambda k: np.mean([r[k] for r in R])
            g = lambda k: np.mean([r["gauges"][k] for r in R])
            gaps = []
            for s in ss:
                lpf = d.get((c, "LP", s))
                if lpf is None:
                    continue
                den = lpf["hol_s0"] - lpf["hol_lp"]
                if den > 1e-9:
                    gaps.append((d[(c, cand, s)]["hol_sum_nonhole"] - lpf["hol_lp"]) / den)
            gp = "%.2f" % np.mean(gaps) if gaps else "-"
            p("| %s | %s | %d | %.5f | %.2f | %.2f | %.1f | %.5f | %.0f (max %d) | %s | %.0f | %.0f | %d |" % (
                c, cand, len(ss), m("G_ratio"), m("hol_p50") * 1e3, m("hol_p99") * 1e3, m("stall_p99") * 1e3,
                m("repair_per_delivered"), m("max_ep_spend"), max(r["max_ep_spend"] for r in R), gp, g("refusals"), g("age_gate_fail"),
                sum(r["gauges"]["p1_viol"] for r in R)))
    p("\n### P5 readings: paired per-seed differences, mean [95% bootstrap CI], seeds in brackets\n")
    p("| config | contrast | metric | diff | CI | n |")
    p("|---|---|---|---|---|---|")
    verdict = {"RA": {"better": [], "worse": []}, "TR": {"better": [], "worse": []}}
    for c in cfgs:
        for name, a, b, key in [("RA", "S2", "S1c", "RA"), ("TR", "S3", "S2", "TR"), ("TR1c", "S3", "S1c", "TR")]:
            ss = sorted(set(seeds(c, a)) & set(seeds(c, b)))
            if not ss:
                continue
            for met, scale in [("hol_p99", 1e3), ("max_ep_spend", 1), ("hol_sum_nonhole", 1)]:
                diff = [(d[(c, a, s)][met] - d[(c, b, s)][met]) * scale for s in ss]
                mu, lo, hi = boot(diff)
                p("| %s | %s - %s | %s | %.4g | [%.4g, %.4g] | %d |" % (c, a, b, met, mu, lo, hi, len(ss)))
                if met in ("hol_p99", "max_ep_spend") and name != "TR1c":
                    if hi < 0 and (met == "max_ep_spend" or True):
                        verdict[key]["better"].append((c, met))
                    if lo > 0:
                        verdict[key]["worse"].append((c, met))
    p("\nrun-atomic: better in %s ; worse in %s" % (verdict["RA"]["better"], verdict["RA"]["worse"]))
    p("T_rho: better in %s ; worse in %s" % (verdict["TR"]["better"], verdict["TR"]["worse"]))
    # LP diagnostics
    p("\n### LP / MILP\n")
    p("| config | seeds | LP s (mean) | LP hol | S0 hol | fractional x (mean) | MILP hol (3 seeds) | MILP-LP gap |")
    p("|---|---|---|---|---|---|---|---|")
    for c in cfgs:
        L = [v for (cc, ca, s), v in d.items() if cc == c and ca == "LP"]
        if not L:
            continue
        M = [v for v in L if "hol_milp" in v]
        p("| %s | %d | %.1f | %.2f | %.2f | %.1f | %s | %s |" % (
            c, len(L), np.mean([v["lp_s"] for v in L]), np.mean([v["hol_lp"] for v in L]), np.mean([v["hol_s0"] for v in L]),
            np.mean([v["n_x_frac"] for v in L]),
            "%.2f" % np.mean([v["hol_milp"] for v in M]) if M else "-",
            "%.3f" % np.mean([v["hol_milp"] - v["hol_lp"] for v in M]) if M else "-"))
    p("\n### Wall time per run (s, mean)\n")
    for c in cfgs:
        w = [r["wall_s"] for (cc, ca, s), r in d.items() if cc == c and ca != "LP" and "wall_s" in r]
        if w:
            p("- %s: %.1f" % (c, np.mean(w)))


CONFIG_ORDER = "I1,I2,G,B10,B60,I1H5,I2H5,GH5,B10H5,I1H5f,I2H5f,GH5f,I1f,I2f,Gf,B10n2s,B10n2f,B10n2a".split(",")
if __name__ == "__main__":
    main()
