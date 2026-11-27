"""Stage-3b runner (three-knobs.md 13.7).  Same structure as batch3.py: seeds one at a time, one file per run (tmp + rename),
existing files are skipped so a killed run resumes.  Several processes may run disjoint seed windows.

    python batch3b.py --part ID|AA|PC|X|K|CL|OV|REST [--seeds 12] [--seed0 0] [--only PREFIX] [--cells A,B] [--list]

Run order of 13.7.5: tests; ID (identity arms); AA (A/A, frozen as nf_stage3b.json by summarize3b.py); placement (pc_place.py)
then PC; X and K (crossover cells); CL and OV; REST (everything else).  Output: out3b/<PART>/<cell>.<arm>-<seed>.json.
"""
import argparse
import json
import math
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import toy2  # noqa: E402
import batch3  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "out3b")
PC_PATH = os.path.join(HERE, "pc_stage3b.json")
NUDGE = batch3.NUDGE
INF = math.inf
NE = batch3.NE
tag = batch3.tag
gname = batch3.gname

LINK_KW = dict(src_model="wc", rtt_est="rfc", resend_gate="evidence", pause_B=1000, J=0.001)
CELLS = dict(batch3.CELLS)
CELLS["E-I2n"] = ("I2", dict(owd=0.005, n_total=NE, dead=(NE - 50, (NE - 50) / 10000.0, 1.5)))
CELLS["CL"] = ("I2", dict(owd=0.005, rate=10000.0, n_total=100000, link=dict(C=12500.0, Q=1250.0), dead=(0, 2.0, 1.5), t_limit=40.0, **LINK_KW))
CELLS["OV"] = ("I2", dict(owd=0.005, rate=12000.0, n_total=120000, link=dict(C=10000.0, Q=1250.0), t_limit=40.0, **LINK_KW))
CELLS["X15"] = ("L10", dict(owd=0.015, rate=50.0, n_total=3000, t_limit=80.0))
CELLS["X20"] = ("L10", dict(owd=0.020, rate=50.0, n_total=3000, t_limit=80.0))

STAGE_CELLS = ("E-I2", "E-L10", "M-I2", "M-L10", "K-I2", "K-L10")
LINK_CELLS = ("CL", "OV")
GRID = [(T, H) for T in (0.4, 2.0, INF) for H in (0.0, INF) if not (T == INF and H == INF)]
SW = {"X15": [round(0.040 + 0.002 * i, 3) for i in range(16)],
      "X20": [round(0.055 + 0.002 * i, 3) for i in range(16)],
      "K-L10": [round(0.150 + 0.005 * i, 3) for i in range(14)],
      "K-L10b": [round(0.180 + 0.005 * i, 3) for i in range(14)]}
CROSS = ("X15", "X20", "K-L10", "K-L10b")
READ_CELLS = ("E-I2", "E-I2n", "E-L10", "M-I2", "M-L10", "K-I2", "K-L10", "K-L10b", "CL", "OV", "X15", "X20")   # A/A cells


def base(cell, **kw):
    lc, ck = CELLS[cell]
    d = dict(ck)
    d.update(kw)
    return lc, d


def cmp_cfg(cell, which, seed):
    lc, d = base(cell)
    if which == "C1":
        return toy2.Cfg(cell=lc, sender="S1c", recv="R0e", seed=seed, delta=50.0, rho=0.99, **d)
    if which == "C3":
        return toy2.Cfg(cell=lc, sender="S1c", recv="R1", seed=seed, delta=50.0, rho=1.0, **d)
    raise KeyError(which)


def s3_cfg(cell, seed, T, H, tail="R", sender="S3", variant=0, **kw):
    """The revised arm (13.7.1): S2 plus evidence-gated verdicts, dequeue expiry, no exclusion from the brake, RFC 6298 RTT.
    S3x (sender="S3x") is the same with the exclusion on (stage 3's brake)."""
    lc, d = base(cell)
    f = dict(evidence_verdict=True, deq_expiry=True, o_excl_verdict=(sender == "S3x"), rtt_est="rfc", tail_elig=tail)
    f.update(kw)
    d.update(f)
    return toy2.Cfg(cell=lc, sender=sender, recv="RH", seed=seed, delta=50.0, rho=1.0, check_p1=False, T=T, Hk=H, variant=variant, **d)


def s2_cfg(cell, seed, T, H, **kw):
    """Stage 3's arm S2 (old sweep), for D1 on a real link and for the old-sweep crossover."""
    lc, d = base(cell)
    d.update(kw)
    return toy2.Cfg(cell=lc, sender="S2", recv="RH", seed=seed, delta=50.0, rho=1.0, check_p1=False, T=T, Hk=H, **d)


JOBS = {}


def job(part, name, fn):
    assert name not in JOBS, name
    JOBS[name] = (part, fn)


def pc_points():
    return json.load(open(PC_PATH)) if os.path.exists(PC_PATH) else {}


def build():
    JOBS.clear()
    pcp = pc_points()
    cells = list(CELLS)
    for cell in cells:
        def J(part, nm, fn, cell=cell):
            job(part, "%s.%s" % (cell, nm), fn)

        # identity arms (every cell): C3 and C3R first; the three identity arms
        J("ID", "C3", lambda s, cell=cell: cmp_cfg(cell, "C3", s))
        J("ID", "C3R", lambda s, cell=cell: s3_cfg(cell, s, INF, INF))                      # S3 (R) at T = inf, H = inf (also A/A variant 0)
        J("ID", "ID-P-inf", lambda s, cell=cell: s3_cfg(cell, s, INF, INF, tail="P"))
        J("ID", "ID-P-1e6", lambda s, cell=cell: s3_cfg(cell, s, 1e6, INF, tail="P"))
        J("ID", "ID-R-1e6", lambda s, cell=cell: s3_cfg(cell, s, 1e6, INF))
        if cell in READ_CELLS:
            J("AA", "AA-1", lambda s, cell=cell: s3_cfg(cell, s, INF, INF, variant=1))
            for var in (2, 3):                                          # unregistered replication of the A/A (as stage 3's REP)
                J("REP", "AA-%d" % var, lambda s, cell=cell, var=var: s3_cfg(cell, s, INF, INF, variant=var))
    for cell in STAGE_CELLS + ("CL",):
        part = "CL" if cell == "CL" else "REST"
        for T, H in GRID:
            job(part, "%s.%s" % (cell, gname(T, H)), lambda s, cell=cell, T=T, H=H: s3_cfg(cell, s, T, H))
        job(part, "%s.C1" % cell, lambda s, cell=cell: cmp_cfg(cell, "C1", s))
        job(part, "%s.%s" % (cell, gname(0.4, 0.02)), lambda s, cell=cell: s3_cfg(cell, s, 0.4, 0.02))
        for x in NUDGE:
            if x != 1.0:
                job(part, "%s.NT-%s" % (cell, tag(x)), lambda s, cell=cell, x=x: s3_cfg(cell, s, 0.4 * x, 0.02))
                job(part, "%s.NH-%s" % (cell, tag(x)), lambda s, cell=cell, x=x: s3_cfg(cell, s, 0.4, 0.02 * x))
    for cell in ("M-I2", "M-L10", "CL", "OV"):                                           # the brake pair
        job("CL" if cell == "CL" else ("OV" if cell == "OV" else "REST"), "%s.S3x" % cell, lambda s, cell=cell: s3_cfg(cell, s, 0.4, INF, sender="S3x"))
    for cell in LINK_CELLS:                                                              # D1 on a real link
        part = cell
        job(part, "%s.S2-T0p4-H0" % cell, lambda s, cell=cell: s2_cfg(cell, s, 0.4, 0.0))
    # OV: the comparison arms and the grid (OV carries no nudges)
    for T, H in GRID:
        job("OV", "OV.%s" % gname(T, H), lambda s, T=T, H=H: s3_cfg("OV", s, T, H))
    job("OV", "OV.C1", lambda s: cmp_cfg("OV", "C1", s))
    # crossover sweeps (H = 0, S3 R) and the T = 0.4 level run
    for cell in CROSS:
        for T in SW[cell]:
            job("K" if cell.startswith("K") else "X", "%s.SW-%s" % (cell, tag(T)), lambda s, cell=cell, T=T: s3_cfg(cell, s, T, 0.0))
        if cell not in STAGE_CELLS:
            job("K" if cell.startswith("K") else "X", "%s.%s" % (cell, gname(0.4, 0.0)), lambda s, cell=cell: s3_cfg(cell, s, 0.4, 0.0))
        if cell in ("K-L10", "K-L10b"):
            for T in SW[cell]:
                job("K", "%s.SW2-%s" % (cell, tag(T)), lambda s, cell=cell, T=T: s2_cfg(cell, s, T, 0.0))
            job("K", "%s.S2-T0p4-H0" % cell, lambda s, cell=cell: s2_cfg(cell, s, 0.4, 0.0))     # the T = 0.4 level of S2's own crossing
    # positive control: T = p * {0.98 ... 1.02}, H = inf, p from the placement step (pc_place.py)
    for cell in ("E-I2n", "E-I2", "E-L10"):
        if cell in pcp:
            for x in NUDGE:
                job("PC", "%s.PC-%s" % (cell, tag(x)), lambda s, cell=cell, x=x: s3_cfg(cell, s, pcp[cell]["p"] * x, INF))


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
    if cfg.n_total and cfg.dead is not None and cfg.dead[0] > 0:               # E cells: delivery ages of the dead tail (placement step)
        d0 = cfg.dead[0]
        out["tail_ages"] = [sim.dlv[s] - sim.h_seq[s] for s in range(d0, sim.n_sent) if sim.dlv[s] >= 0]
        out["tail_undelivered"] = sum(1 for s in range(d0, sim.n_sent) if sim.dlv[s] < 0)
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--part", required=True)
    ap.add_argument("--seeds", type=int, default=12)
    ap.add_argument("--seed0", type=int, default=0)
    ap.add_argument("--only", default="")
    ap.add_argument("--cells", default="")
    ap.add_argument("--list", action="store_true")
    a = ap.parse_args()
    build()
    cells = [c for c in a.cells.split(",") if c]
    names = [n for n, (p, _) in JOBS.items() if p in a.part.split(",") and n.startswith(a.only)
             and (not cells or n.split(".")[0] in cells)]
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
