"""Revision 4 section 10 (the floor repeat): reported-only runs, 3 seeds, on the frozen Revision 4 build and on Revision 3, each with floor_repeat off and "evidenced".

    python rev4_floor.py [--workers 9] [--seeds 3]      rows in out3e/rev4/floor.jsonl; python rev4_floor.py --table
"""
import argparse
import json
import multiprocessing as mp
import os
import sys

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import batch3e as B  # noqa: E402
import rev4_step2 as S  # noqa: E402

INF = float("inf")
FILE = os.path.join(B.OUT, "rev4", "floor.jsonl")
OVEND = dict(rate2=8000.0, rate2_t=6.0, n_total=152000)
CELLS = (  # (label cell, base cell, T, delta, extra kw)
    ("K-OV", "K-OV", 0.4, 50.0, {}), ("OV", "OV", 0.4, 50.0, {}), ("OV", "OV", 2.0, 50.0, {}),
    ("OV-end", "OV", 0.4, 50.0, OVEND), ("OV-end", "OV", 2.0, 0.5, OVEND),
    ("CL-long", "CL-long", 0.4, 50.0, {}), ("CL-long", "CL-long", 2.0, 50.0, {}), ("CL-long", "CL-long", INF, 50.0, {}),
    ("CL", "CL", 0.4, 50.0, {}), ("CL", "CL", 2.0, 50.0, {}), ("CLw-long", "CLw-long", 0.4, 50.0, {}), ("CLw-long", "CLw-long", 2.0, 50.0, {}),
    ("K-I2", "K-I2", 0.12, 50.0, {}), ("K-L10", "K-L10", 0.12, 50.0, {}), ("E-I2", "E-I2", INF, 50.0, {}), ("Ec", "Ec", INF, 50.0, {}),
)


def specs():
    out = []
    for cell, base, T, d, kw in CELLS:
        for build in ("R3", "R4"):
            for fr in ("", "evidenced"):
                r4 = build == "R4"
                lab = "%s | %s T=%s d=%g %s fr=%s" % (cell, "new RL", B.TAG(T), d, build, fr or "off")
                out.append(dict(label=lab, group="floor", cell=cell, base=base, design="new", mode="RL", T=T, delta=d, clamp=True, wait=r4, eq=False, cwnd=0.0, kw=dict(kw),
                                H=0.0, gate=r4, to=0.0, rule="engine" if r4 else "", fr=fr))
    return out


def run():
    ap = argparse.ArgumentParser()
    ap.add_argument("--workers", type=int, default=9)
    ap.add_argument("--seeds", type=int, default=3)
    a = ap.parse_args()
    B.build()
    os.makedirs(os.path.dirname(FILE), exist_ok=True)
    have = set()
    if os.path.exists(FILE):
        for ln in open(FILE, encoding="utf-8"):
            if ln.strip():
                d = json.loads(ln)
                have.add((d["label"], d["seed"]))
    tasks = [(s, k) for k in range(a.seeds) for s in specs() if (s["label"], k) not in have]
    tasks.sort(key=lambda t: -(120 if t[0]["cell"].startswith("CL-long") else 40))
    print("%d runs" % len(tasks), flush=True)
    with mp.Pool(a.workers) as pool, open(FILE, "a", encoding="utf-8") as f:
        for i, row in enumerate(pool.imap_unordered(S.one, tasks), 1):
            f.write(json.dumps(row) + "\n")
            f.flush()
            print("[%d/%d] %s seed %d %.0fs" % (i, len(tasks), row["label"], row["seed"], row["wall_s"]), flush=True)
    print("finished-process", flush=True)


if __name__ == "__main__":
    mp.freeze_support()
    run()
