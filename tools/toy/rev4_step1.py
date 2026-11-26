"""Revision 4, step 1 (three-knobs-revision4.md 5.3): the wait limit's inputs as readouts, on seed 0 of every stored new-model configuration, plus MAIN's CL-long arms
and the D-hat bias rows in CL-long.  Each run has rev4_gauge on; the stored ones are compared with their stored results under id3e (the readouts are inert).

    python rev4_step1.py --reuse-dirs 3d=PATH,ps1=PATH [--workers 9]

Writes out3e/rev4/step1.json and prints the per-cell table.  A non-zero count of ticks with tau < T in a cell with headroom (other than the predicted exceptions RF3, RE and
RL at a 10 % bias) is the stop rule of 5.3.
"""
import argparse
import dataclasses
import json
import multiprocessing as mp
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import batch3e as B  # noqa: E402

OUT = os.path.join(B.OUT, "rev4")
HEADROOM_EXCEPT = ("RF3", "RE")


def tasks():
    """(label, cell, arm, builder(seed) -> Cfg, stored fingerprint or None)."""
    B.build()
    out = []
    seen = set()
    for p in ("AA", "AA2", "DECIDE", "EARLY", "MAIN", "REST"):
        for j in B.PARTS[p]:
            if j.design == "new" and j.fp not in seen and B.have_run(j.fp, 0):
                seen.add(j.fp)
                out.append(("stored", j.cell, "%s | %s" % (j.arm or j.key, B.n_mode(j.mode)), j.builder, j.fp))
    for j in B.PARTS["MAIN"]:                                                # MAIN's CL-long arms (held, not stored): the predicted exceptions RF3 and RE are among them
        if j.cell == "CL-long" and j.fp not in seen:
            seen.add(j.fp)
            out.append(("main-arm", j.cell, "%s" % j.arm, j.builder, None))
    for T in (0.4, 2.0):                                                     # the bias rows: D-hat read low by b in CL-long's drain
        for mode, nm in ((("OFF"), "S5"), (0.0, "R0"), ("RL", "RL")):
            for b in (0.02, 0.05, 0.10):
                out.append(("bias", "CL-long", "%s T=%g bias %.2f" % (nm, T, b), (lambda s, T=T, mode=mode, b=b: B.new_cfg("CL-long", s, T, 0.0, 50.0, B.rp(mode), dhat_bias=b)), None))
    return out


def one(i):
    kind, cell, arm, builder, fp = TASKS[i]
    cfg = dataclasses.replace(builder(0), rev4_gauge=True)
    r = B.run_one(cfg, False)
    inert = None
    if fp is not None:
        old = B.load(fp, 0)
        inert = B.id3e(old, r) if old is not None else None
    return dict(kind=kind, cell=cell, arm=arm, rev4=r["rev4"], inert_diff=inert, finished=r.get("finished"))


TASKS = []


def init(reuse):
    global TASKS
    if reuse:
        B.set_reuse([x for x in reuse.split(",") if x])
    TASKS = tasks()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--reuse-dirs", default="")
    ap.add_argument("--workers", type=int, default=9)
    a = ap.parse_args()
    init(a.reuse_dirs)
    os.makedirs(OUT, exist_ok=True)
    n = len(TASKS)
    print("%d configurations" % n, flush=True)
    res = []
    with mp.Pool(a.workers, initializer=init, initargs=(a.reuse_dirs,)) as pool:
        for k, r in enumerate(pool.imap_unordered(one, range(n)), 1):
            res.append(r)
            print("[%d/%d] %s | %s | n_tau_lt_T %d" % (k, n, r["cell"], r["arm"], r["rev4"]["n_tau_lt_T"]), flush=True)
    with open(os.path.join(OUT, "step1.json"), "w") as f:
        json.dump(res, f)
    cells = {}
    for r in res:
        if r["kind"] == "bias":
            continue
        c = cells.setdefault(r["cell"], dict(n=0, nz=[], ticks=0, tau=0, inert_bad=0, ov=0.0))
        c["n"] += 1
        c["ticks"] += r["rev4"]["n_ticks"]
        c["tau"] += r["rev4"]["n_tau_lt_T"]
        c["ov"] = max(c["ov"], r["rev4"]["ov_frac"])
        if r["rev4"]["n_tau_lt_T"]:
            c["nz"].append(r["arm"])
        if r["inert_diff"]:
            c["inert_bad"] += 1
    print("\n| cell | configurations | ticks | ticks with tau < T | configurations with a non-zero count | max ov_frac | stored rows not inert |\n|---|---|---|---|---|---|---|")
    for cell, c in sorted(cells.items()):
        print("| %s | %d | %d | %d | %s | %.3f | %d |" % (cell, c["n"], c["ticks"], c["tau"], "; ".join(c["nz"]) or "none", c["ov"], c["inert_bad"]))
    print("\nbias rows (CL-long):")
    for r in sorted((r for r in res if r["kind"] == "bias"), key=lambda r: r["arm"]):
        print("  %-26s n_tau_lt_T %6d / ticks %6d  ov_frac %.3f  collapse_t %s" % (r["arm"], r["rev4"]["n_tau_lt_T"], r["rev4"]["n_ticks"], r["rev4"]["ov_frac"], r["rev4"]["collapse_t"]))


if __name__ == "__main__":
    mp.freeze_support()
    main()
