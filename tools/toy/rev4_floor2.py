"""Revision 4 sections 10.7 and 11.2: reported-only runs, 3 seeds, Revision 3 (clamp) and Revision 4 (clamp + wait limit, no gate), each with
(a) today's rules, (b) the floor repeat of 10.3 (evidenced), (c) 10.7 and 11.2 both on (floor_repeat evidenced_overdue, elig rack).

    python rev4_floor2.py [--workers 9] [--seeds 3]      rows in out3e/rev4/floor2.jsonl
"""
import argparse
import json
import multiprocessing as mp
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import batch3e as B  # noqa: E402
import rev4_floor as F  # noqa: E402
import rev4_step2 as S  # noqa: E402

FILE = os.path.join(B.OUT, "rev4", "floor2.jsonl")
VARIANTS = (("off", "", ""), ("evid", "evidenced", ""), ("new", "evidenced_overdue", "rack"))


def specs():
    out = []
    for cell, base, T, d, kw in F.CELLS:
        for build in ("R3", "R4"):
            for vname, fr, elig in VARIANTS:
                r4 = build == "R4"
                lab = "%s | %s T=%s d=%g %s v=%s" % (cell, "new RL", B.TAG(T), d, build, vname)
                out.append(dict(label=lab, group="floor2", cell=cell, base=base, design="new", mode="RL", T=T, delta=d, clamp=True, wait=r4, eq=False, cwnd=0.0, kw=dict(kw),
                                H=0.0, gate=False, to=0.0, rule="", fr=fr, elig=elig))
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
