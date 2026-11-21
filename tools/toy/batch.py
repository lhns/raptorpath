"""Sequential stage-1 runner.  One process, one seed at a time; each run writes its own file
(tmp + rename), and existing files are skipped, so a killed run resumes.

    python batch.py [--seeds 20] [--configs I1,I2,...] [--milp-seeds 3]
"""
import argparse
import json
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import lp  # noqa: E402
import toy  # noqa: E402

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "out")
CANDS = ["S0", "S1", "S1c", "S2", "S3"]
# name -> (cell, topology, Cfg overrides).  Base = N=1 RTT 10, jitter 1 ms FIFO, H = H§8, rho 0.99.
CONFIGS = {
    "I1": ("I1", "n1", {}),
    "I2": ("I2", "n1", {}),
    "G": ("G", "n1", {}),
    "B10": ("B10", "n1", {}),
    "B60": ("B60", "n1", {}),
    "I1H5": ("I1", "n1", {"H": 0.005}),
    "I2H5": ("I2", "n1", {"H": 0.005}),
    "GH5": ("G", "n1", {"H": 0.005}),
    "B10H5": ("B10", "n1", {"H": 0.005}),
    "I1H5f": ("I1", "n1", {"H": 0.005, "floor_with_copy": False}),
    "I2H5f": ("I2", "n1", {"H": 0.005, "floor_with_copy": False}),
    "GH5f": ("G", "n1", {"H": 0.005, "floor_with_copy": False}),
    "B10n2s": ("B10", "n2", {"phi_slow": 0.5, "blackout_path": 1}),   # outage on the slow path, fast path live
    "B10n2f": ("B10", "n2", {"phi_slow": 0.5, "blackout_path": 0}),   # outage on the fast path
    "B10n2a": ("B10", "n2", {"phi_slow": 0.5}),                       # both paths
}
LP_SEEDS = {"I1H5f": 0, "I2H5f": 0, "GH5f": 0, "I2": 6, "G": 6, "I2H5": 3, "GH5": 3, "B10n2s": 6, "B10n2f": 6, "B10n2a": 6, "B10H5": 6}  # LP is 30-60 s where the count binds
DEFAULT = "I1,I2,G,B10,B60,I1H5,I2H5,GH5,B10H5,B10n2s,B10n2f,B10n2a"


def dump(path, obj):
    tmp = path + ".tmp"
    with open(tmp, "w") as f:
        json.dump(obj, f)
    os.replace(tmp, path)


def fname(cfgname, cand, seed):
    return os.path.join(OUT, "%s-%s-%d.json" % (cfgname, cand, seed))


def make(cfgname, cand, seed, **extra):
    cell, topo, kw = CONFIGS[cfgname]
    return toy.make_cfg(cell, cand, seed, topo=topo, **{**kw, **extra})


def clean(r):
    return {k: v for k, v in r.items() if not k.startswith("_")}


def do_group(cfgname, seed, milp_seeds, log):
    need = [c for c in CANDS if not os.path.exists(fname(cfgname, c, seed))]
    lpf = fname(cfgname, "LP", seed)
    need_lp = (not os.path.exists(lpf)) and seed < LP_SEEDS.get(cfgname, 10 ** 9)
    if not need and not need_lp:
        return
    for c in need:
        if c == "S0" and need_lp:
            continue
        t = time.time()
        r = toy.run(make(cfgname, c, seed))
        r["wall_s"] = time.time() - t
        dump(fname(cfgname, c, seed), clean(r))
        log("%s %s %d %.1fs" % (cfgname, c, seed, r["wall_s"]))
    if need_lp or "S0" in need:
        t = time.time()
        r0 = toy.run(make(cfgname, "S0", seed, want_trace=True))
        t_s0 = time.time() - t
        if "S0" in need:
            r = clean(r0)
            r["wall_s"] = t_s0
            dump(fname(cfgname, "S0", seed), r)
        if need_lp:
            dur = r0["dur"]
            a, X, p, st = lp.instance_from_run(r0, dur + 1.0 + 1.0)
            t = time.time()
            v, s0, x = lp.solve(a, X, p, st, 0.99)
            t_lp = time.time() - t
            res = dict(cfg=cfgname, seed=seed, hol_lp=v, hol_s0=s0, n_x=len(x), n_x_frac=sum(1 for q in x.values() if 1e-6 < q < 1 - 1e-6),
                       x_sum=sum(x.values()), lp_s=t_lp, s0_s=t_s0, H=r0["_H"])
            if seed < milp_seeds:
                t = time.time()
                try:
                    vm, _, _ = lp.solve(a, X, p, st, 0.99, integer=True, time_limit=90)
                    res.update(hol_milp=vm, milp_s=time.time() - t)
                except RuntimeError as e:      # time limit without an incumbent
                    res.update(milp_failed=str(e)[:60], milp_s=time.time() - t)
            dump(lpf, res)
            log("%s LP %d lp=%.1fs%s" % (cfgname, seed, t_lp, (" milp=%.1fs" % res["milp_s"]) if "milp_s" in res else ""))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--seeds", type=int, default=20)
    ap.add_argument("--configs", default=DEFAULT)
    ap.add_argument("--milp-seeds", type=int, default=3)
    a = ap.parse_args()
    os.makedirs(OUT, exist_ok=True)
    t0 = time.time()

    def log(m):
        print("[%6.0fs] %s" % (time.time() - t0, m), flush=True)

    for seed in range(a.seeds):
        for cfgname in a.configs.split(","):
            do_group(cfgname, seed, a.milp_seeds, log)
    log("done")


if __name__ == "__main__":
    main()
