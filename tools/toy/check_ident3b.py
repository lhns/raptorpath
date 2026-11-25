"""Stage-3b identity check (three-knobs.md 13.7, Q3b-1 second bullet; stage 3 worker decision 17, now committed).

With every stage 3b field at its default, the current toy2.py must reproduce
  A. the stage 3 commit (39efa6a3) on 34 configurations, on EVERY key of the output including the sender trace and the
     per-seq arrays (want_trace), and the stage 2 commit (c0f33f8e) on the stage 1 and 2 configurations, on every key of the
     old output;
  B. the stored stage 3 outputs (out3/*.json) on a sample of stage 3 runs (call cells, an outage cell), every key but wall_s.
Reference code is taken from git (git archive of tools/toy at the named commit), run in a subprocess, never imported here.

    python check_ident3b.py            # exit 0 = identical
"""
import io
import json
import math
import os
import pickle
import subprocess
import sys
import tarfile
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

STAGE2 = "c0f33f8e"
STAGE3 = "39efa6a3"
INF = math.inf

CHILD = r"""
import pickle, sys
sys.path.insert(0, sys.argv[1])
import toy2
specs = pickle.load(open(sys.argv[2], "rb"))
out = []
for kw in specs:
    r = toy2.run(toy2.Cfg(**kw))
    out.append(r)
pickle.dump(out, open(sys.argv[3], "wb"))
"""


def specs34():
    """34 configurations: (name, Cfg kwargs, stage_of_origin).  Stage 1 and 2 senders and receivers over three cells, streaming
    and object mode, then the stage 3 arms.  Traces on."""
    L = []
    base = dict(want_trace=True)
    for cell in ("I2", "G"):
        for snd in ("S0", "S1", "S1c"):
            for rcv in ("R1", "U", "R0e", "R0c"):
                L.append(("%s-%s-%s" % (cell, snd, rcv), dict(cell=cell, sender=snd, recv=rcv, dur=3.0, seed=1, delta=0.5, rho=0.99, **base), 2))
    L.append(("I1-S1c-R1", dict(cell="I1", sender="S1c", recv="R1", dur=3.0, seed=2, delta=50.0, rho=0.99, **base), 2))
    n = 3000
    dead = (n - 100, (n - 100) / 10000.0, 0.3)
    for snd, rcv in (("S1c", "R1"), ("S1c", "R0e"), ("S1c", "U"), ("S0", "R1")):
        L.append(("obj-%s-%s" % (snd, rcv), dict(cell="I2", sender=snd, recv=rcv, seed=3, n_total=n, dead=dead, delta=50.0, rho=0.99, **base), 2))
    for snd, rcv, kw in (("S2", "RH", dict(T=0.4, Hk=0.0)), ("S2c", "RH", dict(T=0.4, Hk=INF)), ("S2", "R1", dict(T=0.4)),
                         ("S2", "RH", dict(T=1e6, Hk=INF)), ("S2", "RH", dict(T=0.3, Hk=0.02, o_excl_verdict=False))):
        L.append(("s3-%s-%s-%s" % (snd, rcv, kw), dict(cell="I2", sender=snd, recv=rcv, seed=4, n_total=n, dead=dead, rho=1.0, check_p1=False, **kw, **base), 3))
    assert len(L) == 34, len(L)
    return L


def ref_run(commit, specs):
    with tempfile.TemporaryDirectory() as td:
        tar = subprocess.run(["git", "archive", commit + ":tools/toy"], cwd=os.path.join(HERE, "..", ".."), check=True, capture_output=True).stdout
        os.makedirs(os.path.join(td, "toy"))
        tarfile.open(fileobj=io.BytesIO(tar)).extractall(os.path.join(td, "toy"))
        tdir = os.path.join(td, "toy")
        pin, pout, pc = (os.path.join(td, x) for x in ("in.pkl", "out.pkl", "child.py"))
        pickle.dump(specs, open(pin, "wb"))
        open(pc, "w").write(CHILD)
        subprocess.run([sys.executable, pc, tdir, pin, pout], check=True, cwd=td)
        return pickle.load(open(pout, "rb"))


def eq(a, b):
    if isinstance(a, float) and isinstance(b, float):
        return a == b or (math.isnan(a) and math.isnan(b))
    if isinstance(a, dict) and isinstance(b, dict):
        return a.keys() == b.keys() and all(eq(a[k], b[k]) for k in a)
    if isinstance(a, (list, tuple)) and isinstance(b, (list, tuple)):
        return len(a) == len(b) and all(eq(x, y) for x, y in zip(a, b))
    try:
        import numpy as np
        if isinstance(a, np.ndarray) or isinstance(b, np.ndarray):
            return np.array_equal(np.asarray(a), np.asarray(b), equal_nan=True)
    except ImportError:
        pass
    return a == b


def part_a():
    import toy2
    L = specs34()
    specs = [kw for _, kw, _ in L]
    new = [toy2.run(toy2.Cfg(**kw)) for kw in specs]
    bad = 0
    ref3 = ref_run(STAGE3, specs)
    for (nm, _, _), a, b in zip(L, ref3, new):
        if "gauges" in b and "gauges" in a:
            gb = {k: v for k, v in b["gauges"].items() if k in a["gauges"]}      # stage 3b adds gauge keys; every old key must be equal
            extra = set(b["gauges"]) - set(a["gauges"])
            a = dict(a)
            b = dict(b)
            if not eq(a.pop("gauges"), gb) or extra - NEW_GAUGES:
                print("DIFF stage 3 gauges", nm)
                bad += 1
            b.pop("gauges")
        d = [k for k in a if k not in b or not eq(a[k], b[k])]
        if d:
            bad += 1
            print("DIFF vs stage 3", nm, d[:5])
    print("A1: %d configurations against the stage 3 commit %s, every old key, traces and per-seq arrays: %s" % (len(L), STAGE3, "identical" if not bad else "DIFFER"))
    n2 = [(nm, kw) for nm, kw, st in L if st == 2]
    ref2 = ref_run(STAGE2, [kw for _, kw in n2])
    idx = {nm: i for i, (nm, _, _) in enumerate(L)}
    bad2 = 0
    for (nm, _), a in zip(n2, ref2):
        b = new[idx[nm]]
        d = [k for k in a if k != "gauges" and (k not in b or not eq(a[k], b[k]))]
        gd = [k for k in a["gauges"] if k not in b["gauges"] or not eq(a["gauges"][k], b["gauges"][k])]
        if d or gd:
            bad2 += 1
            print("DIFF vs stage 2", nm, d[:5], gd[:5])
    print("A2: %d stage 1 and 2 configurations against the stage 2 commit %s, every key of the old output: %s" % (len(n2), STAGE2, "identical" if not bad2 else "DIFFER"))
    return bad + bad2


# gauge keys that stage 3b may add
NEW_GAUGES = {"probes", "intake_expired_deq", "queue_drop", "queue_drop_in", "queue_drop_out", "resend_by_R", "resend_by_report",
              "resend_by_sweep", "pause_t", "vta"}


def part_b():
    import batch3
    batch3.build()
    out3 = os.path.join(HERE, "out3")
    sample = [("K-L10", "G-T0p4-H0p02", 0), ("K-I2", "G-T0p4-H0", 1), ("K-L10", "C3", 2), ("K-L10", "S2c-H0", 3), ("K-I2", "S2p", 4),
              ("E-I2", "G-T0p4-Hinf", 0), ("M-I2", "G-T0p4-H0", 1), ("E-L10", "C1", 2)]
    bad = 0
    n = 0
    for cell, arm, seed in sample:
        name = "%s.%s" % (cell, arm)
        f = os.path.join(out3, batch3.JOBS[name][0], "%s-%d.json" % (name, seed))
        if not os.path.exists(f):
            print("SKIP (no stored output)", f)
            continue
        old = json.load(open(f))
        new = json.loads(json.dumps(batch3.run_one(batch3.JOBS[name][1](seed)), allow_nan=True))
        new.pop("wall_s", None)
        old.pop("wall_s", None)
        og = old.pop("gauges")
        ng = new.pop("gauges")
        d = [k for k in old if k not in new or not eq(old[k], new[k])]
        dg = [k for k in og if k not in ng or not eq(og[k], ng[k])]
        n += 1
        if d or dg:
            bad += 1
            print("DIFF vs stored stage 3 output", name, seed, d[:5], dg[:5])
    print("B: %d stored stage 3 runs re-run and compared on every stored key: %s" % (n, "identical" if not bad else "DIFFER"))
    return bad + (0 if n else 1)


def main():
    bad = part_a() + part_b()
    print("IDENTITY CHECK:", "PASS" if not bad else "FAIL")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
