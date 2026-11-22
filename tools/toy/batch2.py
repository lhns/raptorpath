"""Sequential stage-2 runner (Parts A, T, B).  One process, seeds one at a time; each run writes its own file
(tmp + rename) and existing files are skipped, so a killed run resumes.

    python batch2.py --part A|T|B [--seeds 12] [--only PREFIX] [--list]
"""
import argparse
import json
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import toy  # noqa: E402
import toy2  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "out2")
RTPROP = 0.010
NUDGE = (0.98, 0.99, 1.0, 1.01, 1.02)
OMEGAS = (0.0, 0.3, 1.0, 3.0, 10.0, 100.0)
CELLS3 = ("I1", "I2", "G")
EPS = {"I1": 0.025, "I2": 0.10, "G": 0.025, "L10": 0.10}


def tag(x):
    return ("%g" % x).replace("+", "")


JOBS = {}      # name -> (part, builder(seed) -> ("sim", Cfg) | ("bridge", sender))


def job(part, name, fn):
    assert name not in JOBS, name
    JOBS[name] = (part, fn)


def ucfg(cell, dial_delta, rho, omega, **kw):
    return lambda seed: ("sim", toy2.Cfg(cell=cell, sender="S1c", recv="U", seed=seed, delta=dial_delta, rho=rho, omega=omega, **kw))


def hz_cal_path():
    return os.path.join(OUT, "hz_cal.json")


def hz_cal(cell):
    """HZ's calibration, computed once per cell from an S0 run (seed 1000, outside the 12): tau1 = median first-repair fill
    time, q99 = its 99th percentile, d = mean d_all.  delta* is placed at Auto exactly (omega is the value that does it)."""
    p = hz_cal_path()
    allc = json.load(open(p)) if os.path.exists(p) else {}
    if cell in allc:
        return allc[cell]
    import numpy as np
    sim = toy2.Sim(toy2.Cfg(cell=cell, sender="S0", seed=1000))
    sim.run()
    f1 = [sim.tfill[s] - sim.tdet[s] for s in sim.tfill if sim.fillc.get(s) == 1 and s in sim.tdet]
    tau1, q99 = float(np.median(f1)), float(np.percentile(f1, 99))
    d = sim.d_sum[0] / sim.d_cnt[0]
    eps = EPS[cell]
    cal = dict(tau1=tau1, q99=q99, d=d, eps=eps, dstar=toy2.DELTA_AUTO, omega=toy2.DELTA_AUTO * tau1 / (d * (1 - eps)), n=len(f1))
    allc[cell] = cal
    os.makedirs(OUT, exist_ok=True)
    with open(p + ".tmp", "w") as f:
        json.dump(allc, f)
    os.replace(p + ".tmp", p)
    return cal


def bridge(sender, seed):
    r1 = toy.run(toy.make_cfg("I1", sender, seed, jitter=0.0, H=0.0, floor_with_copy=False, dur=20.0))
    r2 = toy2.run(toy2.Cfg(cell="I1", sender=sender, seed=seed, J=0.0, dur=20.0, phases=(0.0, 0.0), delta=50.0, rho=0.99))
    keys = ("n_src", "delivered", "n_holes", "hol_sum_nonhole", "iso_p50", "G_final", "max_ep_spend")
    return dict(stage1={k: r1[k] for k in keys},
                stage1_gauges={k: r1["gauges"][k] for k in ("verdicts", "passes", "repair_sends", "floors_ctl", "floor_refresh")},
                stage2={k: r2[k] for k in keys},
                stage2_gauges={k: r2["gauges"][k] for k in ("verdicts", "passes", "repair_sends", "floors", "floor_rep")},
                e_mean=r2["e_mean"], e_p99=r2["e_p99"], hol_p99=r1["hol_p99"])


def tail_cfg(cell, sender, recv, seed, **kw):
    n = 12147
    kw.setdefault("owd", 0.05 if cell == "L10" else 0.005)
    return toy2.Cfg(cell=cell, sender=sender, recv=recv, seed=seed, n_total=n, delta=50.0, rho=0.99,
                    dead=(n - 347, (n - 347) / 10000.0, 1.5), **kw)


def build():
    for cell in CELLS3:                                    # S0 references for the relative bars (not in the registered counts)
        job("REF", "REF-%s" % cell, lambda seed, cell=cell: ("sim", toy2.Cfg(cell=cell, sender="S0", recv="R1", seed=seed)))
    # ---- Part A
    for cell in CELLS3:
        for pt, (dd, rho) in (("RT", (50.0, 0.99)), ("AU", (0.5, 1.0))):
            for arm, var in (("A", 0), ("B", 1)):
                job("A", "AA-%s-%s-%s" % (cell, pt, arm), ucfg(cell, dd, rho, 1.0, variant=var))
    for recv in ("R0e", "R1"):
        for arm, var in (("A", 0), ("B", 1)):
            job("A", "AAT-I2-%s-%s" % (recv, arm),
                lambda seed, recv=recv, var=var: ("sim", tail_cfg("I2", "S1c", recv, seed, variant=var)))
    for pre in ("Realtime", "Auto"):
        p = toy2.PRESETS[pre]
        for h in NUDGE:
            for arm, var in (("A", 0), ("B", 1)):
                job("A", "KP-%s-%s-%s" % (pre, tag(h), arm), ucfg("I1", p * h, toy2.RHO[pre], 1.0, variant=var))
    for om in (1.0, 10.0):
        for pre in ("Bulk", "Auto", "Realtime"):
            job("A", "P4J-%s-%s" % (tag(om), pre), ucfg("I1", toy2.PRESETS[pre], toy2.RHO[pre], om, d_frozen=RTPROP))
    for sender in ("S0", "S1c"):
        job("A", "BR-%s" % sender, lambda seed, sender=sender: ("bridge", sender))
    # ---- Part T
    for cell in ("I1", "I2", "L10"):
        job("T", "REFT-%s" % cell, lambda seed, cell=cell: ("sim", tail_cfg(cell, "S0", "R1", seed)))
        for recv in ("R0e", "R0c", "R1", "U"):
            job("T", "T-%s-%s" % (cell, recv), lambda seed, cell=cell, recv=recv: ("sim", tail_cfg(cell, "S1c", recv, seed)))
        job("T", "T-%s-R0x" % cell, lambda seed, cell=cell: ("sim", tail_cfg(cell, "S1c", "R0e", seed, recv_eps="ewma")))
    # ---- Part B
    for pre in ("Bulk", "Auto", "Realtime"):
        p, rho = toy2.PRESETS[pre], toy2.RHO[pre]
        for om in OMEGAS:
            for h in NUDGE:
                job("B", "U-I1-%s-w%s-%s" % (pre, tag(om), tag(h)), ucfg("I1", p * h, rho, om))
            for cell in ("I2", "G"):
                job("B", "U-%s-%s-w%s-1" % (cell, pre, tag(om)), ucfg(cell, p, rho, om))
        job("B", "UREP-I1-%s" % pre, ucfg("I1", p, rho, 1.0, dsel="rep"))
    for om in (1.0, 10.0):                                 # centre is U-I2-Realtime-w*-1
        for x in (0.98, 0.99, 1.01, 1.02):
            job("B", "RHO-I2-w%s-%s" % (tag(om), tag(x)), ucfg("I2", 50.0, 1.0 - 0.01 * x, om))
    job("B", "P2-rho1", ucfg("I1", 50.0, 1.0, 1.0))
    job("B", "P2-rho1m", ucfg("I1", 50.0, 1.0 - 1e-7, 1.0))
    for h in NUDGE:
        job("B", "HZ-I1-Auto-%s" % tag(h), lambda seed, h=h: ("sim", toy2.Cfg(cell="I1", sender="S1c", recv="HZ", seed=seed, delta=0.5 * h,
                                                                              rho=1.0, hz=hz_cal("I1"))))
    for pl in ("STEP", "SPIKE", "KINK"):
        for h in NUDGE:
            job("B", "PL-%s-%s" % (pl, tag(h)), ucfg("I1", 0.5 * h, 1.0, 1.0, plant=pl, plant_p=0.5))
    dpts = (("Realtime", 50.0), ("Auto", 0.5), ("b115", toy2.delta_at_b(1.15)), ("b13", toy2.delta_at_b(1.3)), ("Bulk", 0.005))
    for cname, cell, q in (("I1", "I1", False), ("I2", "I2", False), ("Q", "I1", True)):
        for nm, dd in dpts:
            for arm in ("RT", "EWMA", "ZERO"):
                job("B", "AG1-%s-%s-%s" % (cname, nm, arm), ucfg(cell, dd, 0.99, 1.0, q=q, d_arm=arm))


def fname(name, seed):
    return os.path.join(OUT, JOBS[name][0], "%s-%d.json" % (name, seed))


def dump(path, obj):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    tmp = path + ".tmp"
    with open(tmp, "w") as f:
        json.dump(obj, f)
    os.replace(tmp, path)


def clean(r):
    return {k: v for k, v in r.items() if not k.startswith("_") and k not in ("p4c", "pass_dt")}


def main():
    build()
    ap = argparse.ArgumentParser()
    ap.add_argument("--part", required=True)
    ap.add_argument("--seeds", type=int, default=12)
    ap.add_argument("--seed0", type=int, default=0)
    ap.add_argument("--only", default="")
    ap.add_argument("--list", action="store_true")
    a = ap.parse_args()
    names = [n for n, (p, _) in JOBS.items() if p in a.part.split(",") and n.startswith(a.only)]
    if a.list:
        print(len(names), "configurations")
        for n in names:
            print(n)
        return
    t0 = time.time()
    total = len(names) * a.seeds
    done = 0
    for seed in range(a.seed0, a.seed0 + a.seeds):          # seeds one at a time, as stage 1
        for n in names:
            f = fname(n, seed)
            if os.path.exists(f):
                done += 1
                continue
            kind, spec = JOBS[n][1](seed)
            t = time.time()
            r = clean(toy2.run(spec)) if kind == "sim" else bridge(spec, seed)
            r["wall_s"] = time.time() - t
            dump(f, r)
            done += 1
            print("[%6.0fs] %d/%d %s %d %.1fs" % (time.time() - t0, done, total, n, seed, r["wall_s"]), flush=True)
    print("[%6.0fs] done" % (time.time() - t0), flush=True)


if __name__ == "__main__":
    main()
