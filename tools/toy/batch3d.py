"""Stage-3d runner (three-knobs.md 17.9).  One file per run (tmp + rename), existing files are skipped, so a killed run resumes.
A pool of workers takes (configuration, seed) pairs, the longest runs first.

    python batch3d.py --part ID|AA|SHORT|LONG|P2|PL|REF|SUP [--seeds 12] [--seed0 0] [--only PREFIX] [--cells A,B] [--workers N] [--list]

Run order of 17.9: tests; check_ident3d.py; ID (identity and liveness arms); AA (A/A, frozen as nf_stage3d.json by summarize3d.py);
then everything else.  Output: out3d/<PART>/<cell>.<arm>-<seed>.json.

Cells: the stage 3c cells (sweep form "tail"), the blocker-form variants "<cell>@b" (sweep_form "blocker", rx_refresh, RFC 6298 RTT
for every arm), CL-long, OV-long and the two-path cell P2.
"""
import argparse
import json
import math
import multiprocessing as mp
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import toy2  # noqa: E402
import batch3  # noqa: E402
import batch3b  # noqa: E402
import batch3c  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "out3d")
NUDGE = batch3.NUDGE
INF = math.inf
tag = batch3.tag
gname = batch3.gname
LINK_KW = batch3c.LINK_KW
PATHS = [dict(owd=0.010, J=0.001), dict(owd=0.060, J=0.001)]

CELLS = {k: (v[0], dict(v[1])) for k, v in batch3c.CELLS.items() if k != "E-I2n"}      # copies: batch3c's own dicts stay untouched
CELLS["CL-long"] = ("I2", dict(owd=0.005, rate=10000.0, n_total=200000, link=dict(C=12500.0, Q=1250.0), dead=(0, 2.0, 1.5), t_limit=80.0, **LINK_KW))
CELLS["OV-long"] = ("I2", dict(owd=0.005, rate=12000.0, n_total=480000, link=dict(C=10000.0, Q=1250.0), t_limit=120.0, **LINK_KW))
_p2 = dict(LINK_KW)
_p2.update(owd=0.010, J=0.001, n_total=12147, paths=PATHS, reorder_rack=True, resend_gate="cooldown", t_limit=40.0)
CELLS["P2"] = ("I2", _p2)
WIN_CELLS = ("CL", "CL-long", "OV", "OV-long", "M-I2", "M-L10")
for _c in WIN_CELLS:
    CELLS[_c][1]["windows"] = True
for _c in ("K-I2", "K-L10", "K-L10b", "CL", "OV", "CL-long", "OV-long"):
    _lc, _kw = CELLS[_c]
    CELLS[_c + "@b"] = (_lc, dict(_kw, sweep_form="blocker", rx_refresh=True, rtt_est="rfc"))

STAGE_CELLS = ("E-I2", "E-L10", "M-I2", "M-L10", "K-I2", "K-L10", "K-L10b")
SHORT = STAGE_CELLS + ("CL", "OV") + ("K-I2@b", "K-L10@b", "K-L10b@b", "CL@b", "OV@b")
LONG = ("CL-long", "OV-long", "CL-long@b", "OV-long@b")
XCELLS = ("X15", "X20")
GRID = [(T, H) for T in (0.4, 2.0, INF) for H in (0.0, INF) if not (T == INF and H == INF)]
SW = batch3c.SW
CROSS = ("X15", "X20", "K-L10", "K-L10b")
P2_T = (0.075, 0.4, INF)
PLANT_CELLS = ("OV", "K-L10")
ALL12 = 12


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


def s5_cfg(cell, seed, T, H, variant=0, **kw):
    """Arm S5 (17.9): S4 with the trigger law, the forward one-way term, the floor re-check and no verdict.  S5v (17.6) = S5 with
    verdict_evidence "feedback_tail" and floor_law "verdict" (the ablation: ADR-0084's verdict floor and the feedback give-up)."""
    lc, d = base(cell)
    f = dict(evidence_verdict=False, verdict_evidence="none", floor_law="protect", floor_carrier="header", deq_expiry=True,
             o_excl_verdict=False, rtt_est="rfc", tail_elig="P", trigger_law="trigger", owd_est="fwd", recheck_floor=True)
    f.update(kw)
    d.update(f)
    return toy2.Cfg(cell=lc, sender="S5", recv="RH", seed=seed, delta=50.0, rho=1.0, check_p1=False, T=T, Hk=H, variant=variant, **d)


def s5v_cfg(cell, seed, T, H):
    return s5_cfg(cell, seed, T, H, verdict_evidence="feedback_tail", floor_law="verdict")


def p2_cfg(seed, T, H, hf, variant=0):
    """P2 arm: hf = "est" (the floor-hold estimator, W = 10 s) or "0" (H_force = 0)."""
    return s5_cfg("P2", seed, T, H, variant=variant, H_force=(None if hf == "est" else 0.0))


JOBS = {}


def job(part, name, fn, nseeds=ALL12):
    assert name not in JOBS, name
    JOBS[name] = (part, fn, nseeds)


def hf_name(T, H, hf):
    return "G-T%s-H%s-F%s" % (tag(T), tag(H), hf)


def build():
    JOBS.clear()
    for cell in SHORT + XCELLS:
        def J(part, nm, fn, cell=cell, ns=ALL12):
            job(part, "%s.%s" % (cell, nm), fn, ns)

        J("ID", "C3", lambda s, cell=cell: cmp_cfg(cell, "C3", s))
        J("ID", "C3S", lambda s, cell=cell: s5_cfg(cell, s, INF, INF))
        J("ID", "ID-1e6", lambda s, cell=cell: s5_cfg(cell, s, 1e6, INF))
        J("AA", "AA-1", lambda s, cell=cell: s5_cfg(cell, s, INF, INF, variant=1))
        if cell in XCELLS:
            for T in SW[cell]:
                J("SHORT", "SW-%s" % tag(T), lambda s, cell=cell, T=T: s5_cfg(cell, s, T, 0.0))
            J("SHORT", gname(0.4, 0.0), lambda s, cell=cell: s5_cfg(cell, s, 0.4, 0.0))
            continue
        for T, H in GRID:
            J("SHORT", gname(T, H), lambda s, cell=cell, T=T, H=H: s5_cfg(cell, s, T, H))
        J("SHORT", "C1", lambda s, cell=cell: cmp_cfg(cell, "C1", s))
        J("SHORT", gname(0.4, 0.02), lambda s, cell=cell: s5_cfg(cell, s, 0.4, 0.02))
        for x in NUDGE:
            if x != 1.0:
                J("SHORT", "NT-%s" % tag(x), lambda s, cell=cell, x=x: s5_cfg(cell, s, 0.4 * x, 0.02))
                J("SHORT", "NH-%s" % tag(x), lambda s, cell=cell, x=x: s5_cfg(cell, s, 0.4, 0.02 * x))
    for cell in ("K-L10", "K-L10b"):                                       # the crossover sweeps (H = 0, whole-tail sweep) and the T = 0.4 level run
        for T in SW[cell]:
            job("SHORT", "%s.SW-%s" % (cell, tag(T)), lambda s, cell=cell, T=T: s5_cfg(cell, s, T, 0.0))
    for cell in ("OV", "OV@b"):                                            # the ablation S5v (17.6)
        for H in (0.0, INF):
            job("SHORT", "%s.S5v-H%s" % (cell, tag(H)), lambda s, cell=cell, H=H: s5v_cfg(cell, s, 0.4, H))
    for cell in PLANT_CELLS:                                               # planted controls routed through T (17.5), H = 0
        for kind in ("STEP", "SPIKE", "KINK"):
            for x in NUDGE:
                job("PL", "%s.PL-%s-%s" % (cell, kind, tag(x)), lambda s, cell=cell, kind=kind, x=x: s5_cfg(cell, s, 0.4 * x, 0.0, plant_T=(kind, 0.4)))
    for cell in ("K-I2", "K-L10", "K-L10b"):                               # Q3d-9's reference: stage 3c's arm S4 (the 3c code path, bit-identical by check_ident3d.py)
        job("REF", "%s.S4ref" % cell, lambda s, cell=cell: batch3c.s4_cfg(cell, s, 0.4, INF))
    for cell in ("OV", "OV@b", "OV-long", "OV-long@b"):                    # SUP (unregistered, labelled as such): S5v with the timeout also for a hole no later send has overtaken
        for H in (0.0, INF):
            job("SUP", "%s.SUP-S5v-H%s" % (cell, tag(H)), lambda s, cell=cell, H=H: s5_cfg(cell, s, 0.4, H, verdict_evidence="feedback_tail_any", floor_law="verdict"))
    for cell in LONG:                                                      # CL-long (17.4) and OV-long (17.8): no nudges, no plants
        job("ID", "%s.C3S" % cell, lambda s, cell=cell: s5_cfg(cell, s, INF, INF), 3)
        job("ID", "%s.ID-1e6" % cell, lambda s, cell=cell: s5_cfg(cell, s, 1e6, INF), 3)
        job("AA", "%s.AA-1" % cell, lambda s, cell=cell: s5_cfg(cell, s, INF, INF, variant=1))
        job("LONG", "%s.C3" % cell, lambda s, cell=cell: cmp_cfg(cell, "C3", s))
        job("LONG", "%s.C1" % cell, lambda s, cell=cell: cmp_cfg(cell, "C1", s))
        arms = [(T, H) for T in (0.4, 2.0) for H in (0.0, INF)] + ([(INF, 0.0)] if cell.startswith("CL") else [])
        for T, H in arms:
            job("LONG", "%s.%s" % (cell, gname(T, H)), lambda s, cell=cell, T=T, H=H: s5_cfg(cell, s, T, H))
        if cell.startswith("OV"):
            for H in (0.0, INF):
                job("LONG", "%s.S5v-H%s" % (cell, tag(H)), lambda s, cell=cell, H=H: s5v_cfg(cell, s, 0.4, H))
        job("LONG", "%s.%s" % (cell, gname(0.4, 0.02)), lambda s, cell=cell: s5_cfg(cell, s, 0.4, 0.02))        # the unplanted nudges (17.9: every cell but X and P2)
        for x in NUDGE:
            if x != 1.0:
                job("LONG", "%s.NT-%s" % (cell, tag(x)), lambda s, cell=cell, x=x: s5_cfg(cell, s, 0.4 * x, 0.02))
                job("LONG", "%s.NH-%s" % (cell, tag(x)), lambda s, cell=cell, x=x: s5_cfg(cell, s, 0.4, 0.02 * x))
    # P2: C3 and S5 at T in {0.075, 0.4, inf} x H in {0, inf} x H_f in {est, 0}
    job("ID", "P2.C3", lambda s: cmp_cfg("P2", "C3", s))
    job("ID", "P2.C3S", lambda s: p2_cfg(s, INF, INF, "est"))
    job("ID", "P2.ID-1e6", lambda s: p2_cfg(s, 1e6, INF, "est"))
    job("AA", "P2.AA-1", lambda s: p2_cfg(s, INF, INF, "est", variant=1))
    for T in P2_T:
        for H in (0.0, INF):
            for hf in ("est", "0"):
                job("P2", "P2.%s" % hf_name(T, H, hf), lambda s, T=T, H=H, hf=hf: p2_cfg(s, T, H, hf))


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
    return batch3b.clean(r)


def work(task):
    n, seed = task
    t = time.time()
    r = run_one(JOBS[n][1](seed))
    r["wall_s"] = time.time() - t
    dump(fname(n, seed), r)
    return n, seed, r["wall_s"]


def weight(n):
    c = n.split(".")[0]
    w = 1.0
    if "long" in c:
        w = 40.0
    elif c.startswith(("CL", "OV")):
        w = 5.0
    if n.endswith(("C1",)):
        w *= 3.0
    return -w


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--part", required=True)
    ap.add_argument("--seeds", type=int, default=12)
    ap.add_argument("--seed0", type=int, default=0)
    ap.add_argument("--only", default="")
    ap.add_argument("--cells", default="")
    ap.add_argument("--workers", type=int, default=1)
    ap.add_argument("--list", action="store_true")
    a = ap.parse_args()
    build()
    cells = [c for c in a.cells.split(",") if c]
    names = [n for n, (p, _, _) in JOBS.items() if p in a.part.split(",") and n.startswith(a.only)
             and (not cells or n.split(".")[0] in cells)]
    if a.list:
        print(len(names), "configurations")
        for n in names:
            print(n)
        return
    tasks = [(n, s) for n in names for s in range(a.seed0, a.seed0 + a.seeds) if s < JOBS[n][2] and not os.path.exists(fname(n, s))]
    tasks.sort(key=lambda t: weight(t[0]))
    t0 = time.time()
    print("%d runs to do (%d configurations)" % (len(tasks), len(names)), flush=True)
    done = 0
    if a.workers <= 1:
        for t in tasks:
            n, s, w = work(t)
            done += 1
            print("[%6.0fs] %d/%d %s %d %.1fs" % (time.time() - t0, done, len(tasks), n, s, w), flush=True)
    else:
        with mp.Pool(a.workers, initializer=build) as p:
            for n, s, w in p.imap_unordered(work, tasks, chunksize=1):
                done += 1
                print("[%6.0fs] %d/%d %s %d %.1fs" % (time.time() - t0, done, len(tasks), n, s, w), flush=True)
    print("[%6.0fs] done" % (time.time() - t0), flush=True)


if __name__ == "__main__":
    mp.freeze_support()
    main()
