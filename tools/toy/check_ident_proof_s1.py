"""Proof stage 1 identity check: with the new fields at their defaults, toy2.py must reproduce the code it was built on
(the base commit, stage 3d re-baselined) on every key of the output, trace and per-seq arrays included (as digests).

A small sample, one process at a time so that it does not compete for memory: seed 0 of a handful of stage 3d configurations (the
C1 / R0e path, C3, S5 at two points, a link cell, the blocker form, two paths), the link cells shortened as the unit tests shorten them.
New keys and new gauges are allowed and counted; every old key must be equal.

    python check_ident_proof_s1.py [BASE_REV]      # default BASE_REV = the first parent of the proof branch's first commit
"""
import io
import os
import pickle
import subprocess
import sys
import tarfile
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
SMALL = {"CL@b": dict(n_total=20000, dead=(0, 0.5, 0.5)), "OV@b": dict(n_total=24000), "CL-long@b": dict(n_total=20000, dead=(0, 0.5, 0.5))}
SAMPLE = ("K-I2.C3", "K-I2.C1", "K-I2.G-T0p4-H0", "K-I2.G-T2-Hinf", "K-L10@b.C1", "K-L10@b.G-T0p4-H0",
          "CL@b.C3", "CL@b.C1", "CL@b.G-T0p4-H0", "OV@b.C1", "OV@b.G-T0p4-Hinf", "CL-long@b.C1", "CL-long@b.G-Tinf-H0",
          "P2.C3", "P2.G-T0p4-H0-Fest", "P2.G-T0p4-Hinf-F0")
SEED = 0


def emit(root, out):
    """Run the sample with the modules under root and pickle the digests."""
    sys.path.insert(0, root)
    import toy2
    import batch3d
    import check_ident3d
    assert os.path.dirname(os.path.abspath(toy2.__file__)) == os.path.abspath(root), toy2.__file__
    batch3d.build()
    res = {}
    for nm in SAMPLE:
        cfg = batch3d.JOBS[nm][1](SEED)
        for k, v in SMALL.get(nm.split(".")[0], {}).items():
            setattr(cfg, k, v)
        cfg.want_trace = True
        res[nm] = check_ident3d.digest(toy2.run(cfg))
        print("ran", nm, flush=True)
    with open(out, "wb") as f:
        pickle.dump(res, f)


def main():
    if len(sys.argv) > 1 and sys.argv[1] == "--emit":
        emit(sys.argv[2], sys.argv[3])
        return
    base = sys.argv[1] if len(sys.argv) > 1 else "e53eab45"
    root = os.path.join(HERE, "..", "..")
    tar = subprocess.run(["git", "archive", base + ":tools/toy"], cwd=root, check=True, capture_output=True).stdout
    sys.path.insert(0, HERE)
    import check_ident3d
    with tempfile.TemporaryDirectory() as td:
        old_dir = os.path.join(td, "old")
        os.makedirs(old_dir)
        tarfile.open(fileobj=io.BytesIO(tar)).extractall(old_dir)
        outs = {}
        for side, d in (("old", old_dir), ("new", HERE)):
            outs[side] = os.path.join(td, side + ".pkl")
            subprocess.run([sys.executable, os.path.abspath(__file__), "--emit", d, outs[side]], check=True, cwd=td)
        a, b = (pickle.load(open(outs[s], "rb")) for s in ("old", "new"))
    bad, nkeys, new_keys = 0, 0, set()
    for nm in SAMPLE:
        for k in a[nm]:
            nkeys += 1
            if k not in b[nm]:
                print("MISSING key", nm, k)
                bad += 1
            elif k == "gauges":
                gb = {g: v for g, v in b[nm]["gauges"].items() if g in a[nm]["gauges"]}
                new_keys |= set(b[nm]["gauges"]) - set(a[nm]["gauges"])
                if not check_ident3d.eq(a[nm]["gauges"], gb):
                    print("DIFF gauges", nm, [g for g in a[nm]["gauges"] if not check_ident3d.eq(a[nm]["gauges"][g], gb.get(g))][:5])
                    bad += 1
            elif not check_ident3d.eq(a[nm][k], b[nm][k]):
                print("DIFF", nm, k)
                bad += 1
        new_keys |= {"key:" + k for k in b[nm] if k not in a[nm]}
    print("base %s; %d configurations, seed %d; %d old keys compared (trace and per-seq arrays as digests); new keys/gauges (allowed): %s" % (
        base, len(SAMPLE), SEED, nkeys, sorted(new_keys)))
    print("IDENTITY CHECK (proof stage 1):", "PASS" if not bad else "FAIL (%d differences)" % bad)
    sys.exit(0 if not bad else 1)


if __name__ == "__main__":
    main()
