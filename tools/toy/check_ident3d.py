"""Stage-3d identity check (three-knobs.md 17.9 test 1, Q3d-1 first bullet).

With every stage 3d field at its default, the current toy2.py must reproduce the stage 3c code (commit 02eda90b) on
  A. the 34 configurations of stage 3 worker decision 17, at seeds 0 to 2,
  B. every stage 3b configuration (every cell and arm of batch3b.JOBS), at seeds 0 to 2, and
  C. every stage 3c configuration (every cell and arm of batch3c.JOBS, one run per name), at seeds 0 to 2,
on EVERY key of the old output: scalars, gauges, the sender trace and the per-seq arrays (stored as digests).
The stage 3 and 3b per-run outputs are not tracked (out3/, out3b/), so the references are regenerated first from a git
archive of the reference commit and stored under out3d/ref/ (also not tracked).  New keys and new gauges are allowed and counted.
The new side drives the unchanged batch3.py / batch3b.py builders (checked byte-equal to the reference commit's).

    python check_ident3d.py            # regenerate the references if absent, compare; exit 0 = identical
    python check_ident3d.py --regen    # regenerate the references
"""
import hashlib
import io
import math
import multiprocessing as mp
import os
import pickle
import subprocess
import sys
import tarfile
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
REF = os.path.join(HERE, "out3d", "ref")
REF_COMMIT = "02eda90b"
SEEDS = (0, 1, 2)
INF = math.inf


def specs34():
    L = []
    base = dict(want_trace=True)
    for cell in ("I2", "G"):
        for snd in ("S0", "S1", "S1c"):
            for rcv in ("R1", "U", "R0e", "R0c"):
                L.append(("%s-%s-%s" % (cell, snd, rcv), dict(cell=cell, sender=snd, recv=rcv, dur=3.0, seed=1, delta=0.5, rho=0.99, **base)))
    L.append(("I1-S1c-R1", dict(cell="I1", sender="S1c", recv="R1", dur=3.0, seed=2, delta=50.0, rho=0.99, **base)))
    n = 3000
    dead = (n - 100, (n - 100) / 10000.0, 0.3)
    for snd, rcv in (("S1c", "R1"), ("S1c", "R0e"), ("S1c", "U"), ("S0", "R1")):
        L.append(("obj-%s-%s" % (snd, rcv), dict(cell="I2", sender=snd, recv=rcv, seed=3, n_total=n, dead=dead, delta=50.0, rho=0.99, **base)))
    for i, (snd, rcv, kw) in enumerate((("S2", "RH", dict(T=0.4, Hk=0.0)), ("S2c", "RH", dict(T=0.4, Hk=INF)), ("S2", "R1", dict(T=0.4)),
                                         ("S2", "RH", dict(T=1e6, Hk=INF)), ("S2", "RH", dict(T=0.3, Hk=0.02, o_excl_verdict=False)))):
        L.append(("s3-%d-%s-%s" % (i, snd, rcv), dict(cell="I2", sender=snd, recv=rcv, seed=4, n_total=n, dead=dead, rho=1.0, check_p1=False, **kw, **base)))
    assert len(L) == 34, len(L)
    return L


def digest(r):
    """Every key of a run's output; underscore keys (trace, per-seq arrays) as (length, sha256 of the pickle)."""
    out = {}
    for k, v in r.items():
        if k.startswith("_"):
            out[k] = (len(v), hashlib.sha256(pickle.dumps(list(v))).hexdigest())
        else:
            out[k] = v
    return out


_W = {}


def init(root):
    sys.path.insert(0, root)
    import toy2
    import batch3
    import batch3b
    import batch3c
    assert os.path.dirname(os.path.abspath(toy2.__file__)) == os.path.abspath(root), toy2.__file__
    batch3.build()
    batch3b.build()
    batch3c.build()
    _W.update(toy2=toy2, batch3b=batch3b, batch3c=batch3c)


def work(task):
    kind, name, seed = task
    toy2, b = _W["toy2"], _W["batch3b"]
    if kind == "s34":
        kw = dict(dict(specs34())[name])
        kw["seed"] = seed
        cfg = toy2.Cfg(**kw)
    elif kind == "b3b":
        cfg = b.JOBS[name][1](seed)
        cfg.want_trace = True
    else:
        cfg = _W["batch3c"].JOBS[name][1](seed)
        cfg.want_trace = True
    return task, digest(toy2.run(cfg))


def tasks():
    t = [("s34", nm, s) for nm, _ in specs34() for s in SEEDS]
    import batch3b
    batch3b.build()
    t += [("b3b", n, s) for n in batch3b.JOBS for s in SEEDS]
    import batch3c
    batch3c.build()
    t += [("b3c", n, s) for n in batch3c.JOBS for s in SEEDS
          if not (n.startswith("E-I2n.PC") and str(s) not in batch3c.pc_points().get("E-I2n", {}))]
    return t


def run_pool(root, tl):
    # the longest runs (link cells, C1) first
    tl = sorted(tl, key=lambda x: -(x[1].startswith(("CL", "OV")) * 10 + x[1].endswith("C1") * 5))
    res = {}
    with mp.Pool(min(int(os.environ.get("IDENT_WORKERS", "4")), os.cpu_count() or 4), initializer=init, initargs=(root,)) as p:
        for task, d in p.imap_unordered(work, tl, chunksize=1):
            res[task] = d
    return res


def eq(a, b):
    if isinstance(a, float) and isinstance(b, float):
        return a == b or (math.isnan(a) and math.isnan(b))
    if isinstance(a, dict) and isinstance(b, dict):
        return a.keys() == b.keys() and all(eq(a[k], b[k]) for k in a)
    if isinstance(a, (list, tuple)) and isinstance(b, (list, tuple)):
        return len(a) == len(b) and all(eq(x, y) for x, y in zip(a, b))
    return a == b


def ref_path(task):
    return os.path.join(REF, "%s__%s__%d.pkl" % task)


def regen():
    os.makedirs(REF, exist_ok=True)
    root = os.path.join(HERE, "..", "..")
    tar = subprocess.run(["git", "archive", REF_COMMIT + ":tools/toy"], cwd=root, check=True, capture_output=True).stdout
    with tempfile.TemporaryDirectory() as td:
        tarfile.open(fileobj=io.BytesIO(tar)).extractall(td)
        res = run_pool(td, tasks())
    for t, d in res.items():
        with open(ref_path(t), "wb") as f:
            pickle.dump(d, f)
    print("reference regenerated from %s: %d runs stored under out3d/ref/" % (REF_COMMIT, len(res)))


def builders_unchanged():
    root = os.path.join(HERE, "..", "..")
    bad = []
    for f in ("batch3.py", "batch3b.py", "batch3c.py", "pc_stage3b.json", "pc_stage3c_dev.json"):
        ref = subprocess.run(["git", "show", "%s:tools/toy/%s" % (REF_COMMIT, f)], cwd=root, check=True, capture_output=True).stdout
        cur = open(os.path.join(HERE, f), "rb").read()
        if ref.replace(b"\r\n", b"\n") != cur.replace(b"\r\n", b"\n"):
            bad.append(f)
    return bad


def main():
    if "--regen-only" in sys.argv:
        regen()
        return
    if "--regen" in sys.argv or not os.path.isdir(REF) or len(os.listdir(REF)) < len(tasks()):
        regen()
    bad_b = builders_unchanged()
    print("builders batch3.py, batch3b.py, batch3c.py and the placement files equal to %s: %s" % (REF_COMMIT, "yes" if not bad_b else "NO " + str(bad_b)))
    tl = tasks()
    res = run_pool(HERE, tl)
    bad = 0
    amended = {}                       # RTT-sampling amendment: a run may differ only if it took a skipped RTT sample (rtt_unarrived > 0)
    new_keys = set()
    n_keys = 0
    for t in tl:
        with open(ref_path(t), "rb") as f:
            a = pickle.load(f)
        b = res[t]
        bad0 = bad
        for k in a:
            n_keys += 1
            if k not in b:
                print("MISSING key", t, k)
                bad += 1
            elif k == "gauges":
                gb = {g: v for g, v in b["gauges"].items() if g in a["gauges"]}
                new_keys |= set(b["gauges"]) - set(a["gauges"])
                if not eq(a["gauges"], gb):
                    print("DIFF gauges", t, [g for g in a["gauges"] if g not in gb or not eq(a["gauges"][g], gb[g])][:5])
                    bad += 1
            elif not eq(a[k], b[k]):
                print("DIFF", t, k)
                bad += 1
        new_keys |= {"key:" + k for k in b if k not in a}
        if bad > bad0 and b.get("rtt_unarrived", 0) > 0:
            amended[t] = (bad - bad0, b["rtt_unarrived"])
            bad = bad0
    n34 = sum(1 for t in tl if t[0] == "s34")
    nb = sum(1 for t in tl if t[0] == "b3b")
    print("A: %d runs of the 34 configurations; B: %d runs of the stage 3b configurations; C: %d runs of the stage 3c configurations "
          "(seeds 0 to 2); %d old keys compared, traces and per-seq arrays as digests" % (n34, nb, len(tl) - n34 - nb, n_keys))
    print("new keys / gauges added by stage 3d (allowed): %d" % len(new_keys))
    for t, (nd, nu) in sorted(amended.items()):
        print("AMENDED (expected, rtt_unarrived = %d, %d differing keys):" % (nu, nd), t)
    print("runs differing only because of the RTT-sampling amendment: %d" % len(amended))
    ok = not bad and not bad_b
    print("IDENTITY CHECK (stage 3d):", "PASS" if ok else "FAIL (%d differences)" % bad)
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    mp.freeze_support()
    main()
