"""Stage-3c runner (three-knobs.md 15.6).  Same structure as batch3b.py: seeds one at a time, one file per run (tmp + rename),
existing files are skipped so a killed run resumes.  Several processes may run disjoint seed windows.

    python batch3c.py --part ID|AA|PC|X|K|CL|OV|REST|REP [--seeds 12] [--seed0 0] [--only PREFIX] [--cells A,B] [--list]

Run order of 15.6.3: tests; ID (identity and liveness arms); AA (A/A, frozen as nf_stage3c.json by summarize3c.py); placement
(pc_place3c.py, per seed) then PC; everything else.  Output: out3c/<PART>/<cell>.<arm>-<seed>.json.
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
import batch3b  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "out3c")
PC_PATH = os.path.join(HERE, "pc_stage3c_dev.json")       # the deviation placement (pc_place3c.py --dev); the literal one, pc_stage3c.json, placed no seed
NUDGE = batch3.NUDGE
INF = math.inf
NE = batch3.NE
tag = batch3.tag
gname = batch3.gname

# every arm in the link cells: the work-conserving source, RFC 6298 RTT, the engine's per-seq resend cooldown and resend budget (15.4, 15.6.2)
LINK_KW = dict(src_model="wc", rtt_est="rfc", resend_gate="cooldown", resend_brake="engine", pause_B=1000, J=0.001)
CELLS = dict(batch3.CELLS)
CELLS["E-I2n"] = batch3b.CELLS["E-I2n"]
CELLS["CL"] = ("I2", dict(owd=0.005, rate=10000.0, n_total=100000, link=dict(C=12500.0, Q=1250.0), dead=(0, 2.0, 1.5), t_limit=40.0, **LINK_KW))
CELLS["OV"] = ("I2", dict(owd=0.005, rate=12000.0, n_total=120000, link=dict(C=10000.0, Q=1250.0), t_limit=40.0, **LINK_KW))
CELLS["X15"] = batch3b.CELLS["X15"]
CELLS["X20"] = batch3b.CELLS["X20"]

STAGE_CELLS = batch3b.STAGE_CELLS
LINK_CELLS = ("CL", "OV")
GRID = [(T, H) for T in (0.4, 2.0, INF) for H in (0.0, INF) if not (T == INF and H == INF)]
SW = batch3b.SW
CROSS = batch3b.CROSS
READ_CELLS = batch3b.READ_CELLS
S4B_CELLS = ("OV", "CL", "K-L10")
NO_NUDGE = ("X15", "X20")


def base(cell, **kw):
    lc, ck = CELLS[cell]
    d = dict(ck)
    d.update(kw)
    return lc, d


def cmp_cfg(cell, which, seed, **kw):
    lc, d = base(cell, **kw)
    if which == "C1":
        return toy2.Cfg(cell=lc, sender="S1c", recv="R0e", seed=seed, delta=50.0, rho=0.99, **d)
    if which == "C3":
        return toy2.Cfg(cell=lc, sender="S1c", recv="R1", seed=seed, delta=50.0, rho=1.0, **d)
    raise KeyError(which)


def s4_cfg(cell, seed, T, H, carrier="header", variant=0, **kw):
    """The revised arm S4 (15.6.1): S3 with the old tail sweep, verdicts on receiver feedback only (no verdict timer), and the
    protected floor.  carrier "datagram" is S4b."""
    lc, d = base(cell)
    f = dict(evidence_verdict=False, verdict_evidence="feedback", floor_law="protect", floor_carrier=carrier, deq_expiry=True,
             o_excl_verdict=False, rtt_est="rfc", tail_elig="P")
    f.update(kw)
    d.update(f)
    return toy2.Cfg(cell=lc, sender="S4", recv="RH", seed=seed, delta=50.0, rho=1.0, check_p1=False, T=T, Hk=H, variant=variant, **d)


JOBS = {}


def job(part, name, fn):
    assert name not in JOBS, name
    JOBS[name] = (part, fn)


def pc_points():
    return json.load(open(PC_PATH)) if os.path.exists(PC_PATH) else {}


def build():
    JOBS.clear()
    pcp = pc_points()
    for cell in CELLS:
        def J(part, nm, fn, cell=cell):
            job(part, "%s.%s" % (cell, nm), fn)

        J("ID", "C3", lambda s, cell=cell: cmp_cfg(cell, "C3", s))
        J("ID", "C3S", lambda s, cell=cell: s4_cfg(cell, s, INF, INF))                         # S4 at T = inf, H = inf (also A/A variant 0)
        J("ID", "ID-1e6", lambda s, cell=cell: s4_cfg(cell, s, 1e6, INF))
        if cell in READ_CELLS:
            J("AA", "AA-1", lambda s, cell=cell: s4_cfg(cell, s, INF, INF, variant=1))
            for var in (2, 3):                                                                 # unregistered replication of the A/A
                J("REP", "AA-%d" % var, lambda s, cell=cell, var=var: s4_cfg(cell, s, INF, INF, variant=var))
    for cell in STAGE_CELLS + ("K-L10b", "E-I2n") + LINK_CELLS:
        part = cell if cell in LINK_CELLS else "REST"
        for T, H in GRID:
            job(part, "%s.%s" % (cell, gname(T, H)), lambda s, cell=cell, T=T, H=H: s4_cfg(cell, s, T, H))
        job(part, "%s.C1" % cell, lambda s, cell=cell: cmp_cfg(cell, "C1", s))
        job(part, "%s.%s" % (cell, gname(0.4, 0.02)), lambda s, cell=cell: s4_cfg(cell, s, 0.4, 0.02))
        for x in NUDGE:
            if x != 1.0:
                job(part, "%s.NT-%s" % (cell, tag(x)), lambda s, cell=cell, x=x: s4_cfg(cell, s, 0.4 * x, 0.02))
                job(part, "%s.NH-%s" % (cell, tag(x)), lambda s, cell=cell, x=x: s4_cfg(cell, s, 0.4, 0.02 * x))
    for cell in S4B_CELLS:                                                                     # S4b: the floor as a datagram (option b)
        part = cell if cell in LINK_CELLS else "K"
        job(part, "%s.S4b" % cell, lambda s, cell=cell: s4_cfg(cell, s, 0.4, INF, carrier="datagram"))
    for cell in CROSS:                                                                         # crossover sweeps (H = 0, S4, old sweep) and the T = 0.4 level run
        part = "K" if cell.startswith("K") else "X"
        for T in SW[cell]:
            job(part, "%s.SW-%s" % (cell, tag(T)), lambda s, cell=cell, T=T: s4_cfg(cell, s, T, 0.0))
        if cell not in STAGE_CELLS and cell != "K-L10b":
            job(part, "%s.%s" % (cell, gname(0.4, 0.0)), lambda s, cell=cell: s4_cfg(cell, s, 0.4, 0.0))
    # per-seed positive control in E-I2n: T = p_seed * {0.98 ... 1.02}, H = inf (pc_place3c.py)
    for x in NUDGE:
        job("PC", "E-I2n.PC-%s" % tag(x), lambda s, x=x: s4_cfg("E-I2n", s, pcp["E-I2n"][str(s)]["p"] * x, INF))


def fname(name, seed):
    return os.path.join(OUT, JOBS[name][0], "%s-%d.json" % (name, seed))


def dump(path, obj):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    tmp = path + ".tmp%d" % os.getpid()
    with open(tmp, "w") as f:
        json.dump(obj, f)
    os.replace(tmp, path)


def run_one(cfg):
    sim = toy2.Sim(cfg)
    r = sim.run()
    out = batch3b.clean(r)
    if cfg.n_total and cfg.dead is not None and cfg.dead[0] > 0:                 # E cells: delivery ages of the dead tail (placement)
        d0 = cfg.dead[0]
        out["tail_ages"] = [sim.dlv[s] - sim.h_seq[s] for s in range(d0, sim.n_sent) if sim.dlv[s] >= 0]
        out["tail_undelivered"] = sum(1 for s in range(d0, sim.n_sent) if sim.dlv[s] < 0)
        # ages above 0.2 s of every delivered unit, to rebuild share_T(T) at T above 0.2 s (placement, K-hat and P-hat)
        out["late_ages"] = [sim.dlv[s] - sim.h_seq[s] for s in range(sim.n_sent) if sim.dlv[s] >= 0 and sim.dlv[s] - sim.h_seq[s] > 0.2]
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
            if n.startswith("E-I2n.PC") and str(seed) not in pc_points().get("E-I2n", {}):
                done += 1
                continue                                                        # seed not usable (ramp too wide): no control run
            t = time.time()
            r = run_one(JOBS[n][1](seed))
            r["wall_s"] = time.time() - t
            dump(f, r)
            done += 1
            print("[%6.0fs] %d/%d %s %d %.1fs" % (time.time() - t0, done, total, n, seed, r["wall_s"]), flush=True)
    print("[%6.0fs] done" % (time.time() - t0), flush=True)


if __name__ == "__main__":
    main()
