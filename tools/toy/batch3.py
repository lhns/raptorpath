"""Stage-3 runner (three-knobs.md 9 and 11).  Same structure as batch2.py: seeds one at a time, one file per run (tmp + rename),
existing files are skipped so a killed run resumes.  Several processes may run disjoint seed windows.

    python batch3.py --part ID|AA|K|EM|O [--seeds 12] [--seed0 0] [--only PREFIX] [--list]

Parts follow the run order of section 11 item 4: ID (the P1/P2 identity arms, all cells), AA (the call-cell A/A, frozen before
any decision cell), K (call cells), EM (outage cells), O (overload cell).
"""
import argparse
import json
import math
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import toy2  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "out3")
NUDGE = (0.98, 0.99, 1.0, 1.01, 1.02)
INF = math.inf
NE = 12147                                     # the object of stage 2's Part T
K_WINDOW = (19.6, 21.5)                        # P8: units handed over outside the outage [20.0, 21.5) (0.4 s before it starts)


def tag(x):
    return "inf" if x == INF else ("%g" % x).replace("+", "").replace(".", "p")


# cell -> (loss cell, Cfg keywords)
CELLS = {
    "E-I2": ("I2", dict(owd=0.005, n_total=NE, dead=(NE - 347, (NE - 347) / 10000.0, 1.5))),
    "E-L10": ("L10", dict(owd=0.05, n_total=NE, dead=(NE - 347, (NE - 347) / 10000.0, 1.5))),
    "M-I2": ("I2", dict(owd=0.005, n_total=NE, dead=(0, 0.4 * NE / 10000.0, 1.5))),
    "M-L10": ("L10", dict(owd=0.05, n_total=NE, dead=(0, 0.4 * NE / 10000.0, 1.5))),
    "K-I2": ("I2", dict(owd=0.005, rate=50.0, n_total=3000, dead=(0, 20.0, 1.5), t_limit=80.0)),
    "K-L10": ("L10", dict(owd=0.05, rate=50.0, n_total=3000, dead=(0, 20.0, 1.5), t_limit=80.0)),
    "K-L10b": ("L10", dict(owd=0.06, rate=50.0, n_total=3000, dead=(0, 20.0, 1.5), t_limit=80.0)),
}
CALL = ("K-I2", "K-L10")
OUTAGE = ("E-I2", "E-L10", "M-I2", "M-L10")
SWEEP = {"K-L10": (0.10, 0.14, 0.16, 0.17, 0.18, 0.20, 0.22, 0.25, 0.30, 0.40),
         "K-L10b": (0.10, 0.14, 0.16, 0.17, 0.18, 0.20, 0.22, 0.25, 0.30, 0.40),
         "K-I2": (0.010, 0.020, 0.030, 0.035, 0.040, 0.050, 0.10)}
# overload cell O-I2 (11 item 5): filled in by the O part (rate set from a measured capacity); see build()
O_CELL = {}


def base(cell, **kw):
    lc, ck = CELLS[cell] if cell in CELLS else O_CELL[cell]
    d = dict(ck)
    d.update(kw)
    return lc, d


def cmp_cfg(cell, which, seed):
    lc, d = base(cell)
    if which == "C1":
        return toy2.Cfg(cell=lc, sender="S1c", recv="R0e", seed=seed, delta=50.0, rho=0.99, **d)
    if which == "C2":
        return toy2.Cfg(cell=lc, sender="S1c", recv="R1", seed=seed, delta=50.0, rho=0.99, **d)
    if which == "C3":
        return toy2.Cfg(cell=lc, sender="S1c", recv="R1", seed=seed, delta=50.0, rho=1.0, **d)
    raise KeyError(which)


def new_cfg(cell, seed, T, H, sender="S2", recv="RH", variant=0, **kw):
    lc, d = base(cell)
    return toy2.Cfg(cell=lc, sender=sender, recv=recv, seed=seed, delta=50.0, rho=1.0, check_p1=False, T=T, Hk=H,
                    variant=variant, **d, **kw)


JOBS = {}      # name -> (part, builder(seed) -> Cfg)


def job(part, name, fn):
    assert name not in JOBS, name
    JOBS[name] = (part, fn)


def gname(T, H):
    return "G-T%s-H%s" % (tag(T), tag(H))


def nudge_names(cell, which):
    """The five names of a nudge set (the centre is the grid cell T = 0.4, H = 0.02)."""
    if which == "T":
        return ["NT-%s" % tag(x) if x != 1.0 else gname(0.4, 0.02) for x in NUDGE]
    if which == "H":
        return ["NH-%s" % tag(x) if x != 1.0 else gname(0.4, 0.02) for x in NUDGE]
    if which == "PC":
        return ["PC-%s" % tag(x) for x in NUDGE]
    raise KeyError(which)


def build():
    for cell in CELLS:
        if cell == "K-L10b":
            part_main = "K"
        else:
            part_main = "K" if cell in CALL else "EM"

        def J(part, nm, fn, cell=cell):
            job(part, "%s.%s" % (cell, nm), fn)

        if cell == "K-L10b":                                # the P5 shift check only: the round sweep
            for T in SWEEP[cell]:
                J("K", "SW-%s" % tag(T), lambda s, cell=cell, T=T: new_cfg(cell, s, T, 0.0))
            continue
        # identity arms first (run order, 11 item 4)
        J("ID", "C3", lambda s, cell=cell: cmp_cfg(cell, "C3", s))
        J("ID", gname(INF, INF), lambda s, cell=cell: new_cfg(cell, s, INF, INF))
        J("ID", "S2L", lambda s, cell=cell: new_cfg(cell, s, 1e6, INF))
        J("ID", "S2R1", lambda s, cell=cell: new_cfg(cell, s, 0.4, 0.0, recv="R1"))
        J("ID", gname(0.4, INF), lambda s, cell=cell: new_cfg(cell, s, 0.4, INF))
        J("ID", gname(0.4, 0.0), lambda s, cell=cell: new_cfg(cell, s, 0.4, 0.0))
        if cell in CALL:
            for var in (0, 1):
                J("AA", "AA-%d" % var, lambda s, cell=cell, var=var: new_cfg(cell, s, 0.4, 0.02, variant=var))
        if cell in CALL:                                    # replication of the A/A with two fresh phase draws (as stage 2)
            for var in (2, 3):
                J("REP", "AA-%d" % var, lambda s, cell=cell, var=var: new_cfg(cell, s, 0.4, 0.02, variant=var))
        for w in ("C1", "C2"):
            J(part_main, w, lambda s, cell=cell, w=w: cmp_cfg(cell, w, s))
        for T in (0.4, 2.0, INF):
            for H in (0.0, 0.02, INF):
                nm = gname(T, H)
                if "%s.%s" % (cell, nm) not in JOBS:
                    J(part_main, nm, lambda s, cell=cell, T=T, H=H: new_cfg(cell, s, T, H))
        J(part_main, "S2c-H0", lambda s, cell=cell: new_cfg(cell, s, 0.4, 0.0, sender="S2c"))
        J(part_main, "S2c-Hinf", lambda s, cell=cell: new_cfg(cell, s, 0.4, INF, sender="S2c"))
        J(part_main, "S2p", lambda s, cell=cell: new_cfg(cell, s, 0.4, INF, o_excl_verdict=False))
        for x in NUDGE:
            if x != 1.0:
                J(part_main, "NT-%s" % tag(x), lambda s, cell=cell, x=x: new_cfg(cell, s, 0.4 * x, 0.02))
                J(part_main, "NH-%s" % tag(x), lambda s, cell=cell, x=x: new_cfg(cell, s, 0.4, 0.02 * x))
        if cell in ("E-I2", "E-L10"):
            for x in NUDGE:
                J(part_main, "PC-%s" % tag(x), lambda s, cell=cell, x=x: new_cfg(cell, s, 1.5 * x, INF))
        if cell in SWEEP:
            for T in SWEEP[cell]:
                J(part_main, "SW-%s" % tag(T), lambda s, cell=cell, T=T: new_cfg(cell, s, T, 0.0))


def add_overload(rate, dur=10.0, name="O-I2", part="O"):
    """O-I2: cell I2 at `rate` units/s for `dur` s; T = 0.4, H = 0 against C1.  O3-I2 is the same at 3 s (C1 at 10 s did not finish)."""
    O_CELL[name] = ("I2", dict(owd=0.005, rate=rate, n_total=int(rate * dur), t_limit=dur + 30.0))
    job(part, "%s.C1" % name, lambda s: cmp_cfg(name, "C1", s))
    job(part, "%s.%s" % (name, gname(0.4, 0.0)), lambda s: new_cfg(name, s, 0.4, 0.0))
    job(part, "%s.%s" % (name, gname(0.4, INF)), lambda s: new_cfg(name, s, 0.4, INF))
    job(part, "%s.S2p" % name, lambda s: new_cfg(name, s, 0.4, INF, o_excl_verdict=False))
    job(part, "%s.S2p-H0" % name, lambda s: new_cfg(name, s, 0.4, 0.0, o_excl_verdict=False))     # unregistered: S2p with the P11 arm's H


def fname(name, seed):
    return os.path.join(OUT, JOBS[name][0], "%s-%d.json" % (name, seed))


def dump(path, obj):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    tmp = path + ".tmp%d" % os.getpid()
    with open(tmp, "w") as f:
        json.dump(obj, f)
    os.replace(tmp, path)


DROP = ("p4c", "pass_dt", "tail_verdicted", "tail_v", "tail_res")


def clean(r):
    return {k: v for k, v in r.items() if not k.startswith("_") and k not in DROP}


def run_one(cfg):
    sim = toy2.Sim(cfg)
    r = sim.run()
    out = clean(r)
    # P8: shares over units handed over outside the call cells' outage window, from the per-unit record
    n = sim.n_sent
    dt = sim.dt
    lo, hi = K_WINDOW
    units_out = [u for u in range(sim.units_total) if not (lo <= u * dt < hi)]
    out["win_out_units"] = len(units_out)
    by = {0.4: 0, 2.0: 0}
    anyd = 0
    for s in range(n):
        h = sim.h_seq[s]
        if lo <= h < hi or sim.dlv[s] < 0:
            continue
        anyd += 1
        for k in by:
            by[k] += int(sim.dlv[s] - h <= k)
    out["win_out_any"] = anyd / max(len(units_out), 1)
    out["win_out_by0p4"] = by[0.4] / max(len(units_out), 1)
    out["win_out_by2"] = by[2.0] / max(len(units_out), 1)
    return out


def main():
    build()
    ap = argparse.ArgumentParser()
    ap.add_argument("--part", required=True)
    ap.add_argument("--seeds", type=int, default=12)
    ap.add_argument("--seed0", type=int, default=0)
    ap.add_argument("--only", default="")
    ap.add_argument("--list", action="store_true")
    ap.add_argument("--orate", type=float, default=0.0, help="O part: source rate, units/s")
    a = ap.parse_args()
    if "O" in a.part.split(","):
        add_overload(a.orate)
    if "O3" in a.part.split(","):
        add_overload(a.orate, 3.0, "O3-I2", "O3")
    names = [n for n, (p, _) in JOBS.items() if p in a.part.split(",") and n.startswith(a.only)]
    if a.list:
        print(len(names), "configurations")
        for n in names:
            print(n)
        return
    t0 = time.time()
    total = len(names) * a.seeds
    done = 0
    for seed in range(a.seed0, a.seed0 + a.seeds):
        for n in names:
            f = fname(n, seed)
            if os.path.exists(f):
                done += 1
                continue
            t = time.time()
            r = run_one(JOBS[n][1](seed))
            r["wall_s"] = time.time() - t
            dump(f, r)
            done += 1
            print("[%6.0fs] %d/%d %s %d %.1fs" % (time.time() - t0, done, total, n, seed, r["wall_s"]), flush=True)
    print("[%6.0fs] done" % (time.time() - t0), flush=True)


if __name__ == "__main__":
    main()
