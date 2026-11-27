"""Prints the step 2 sizing table (mean over seeds) from out3e/rev4/step2.jsonl.  Reported only."""
import json
import os
import sys

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
FILE = os.path.join(HERE, "out3e", "rev4", "step2.jsonl")


def load():
    rows = {}
    for ln in open(FILE, encoding="utf-8"):
        if ln.strip():
            d = json.loads(ln)
            rows.setdefault(d["label"], []).append(d)
    return rows


def mean(rs, f):
    v = []
    for r in rs:
        try:
            x = f(r)
        except (KeyError, TypeError, IndexError):
            x = None
        if x is not None and x == x:
            v.append(x)
    return float(np.mean(v)) if v else None


def fmt(x, nd=3):
    return "-" if x is None else ("%.*f" % (nd, x))


def main():
    only = sys.argv[1] if len(sys.argv) > 1 else ""
    rows = load()
    print("| row | n | cwnd_sim | delivered | on time 0.4 | dh p50 | win delivered | win on time | spare_bind | tau<T ticks | collapse_t | floor_bind | repair sent | queue drops | fq evict | recovery_time |")
    print("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|")
    for lab in sorted(rows):
        if not lab.startswith(only):
            continue
        rs = rows[lab]
        wins = []
        for k in range(max(len(r["win"]) for r in rs)):
            wins.append("%s / %s" % (fmt(mean(rs, lambda r, k=k: r["win"][k]["delivered"])), fmt(mean(rs, lambda r, k=k: r["win"][k]["on_time"]))))
        wd = [fmt(mean(rs, lambda r, k=k: r["win"][k]["delivered"])) for k in range(len(rs[0]["win"]))]
        wo = [fmt(mean(rs, lambda r, k=k: r["win"][k]["on_time"])) for k in range(len(rs[0]["win"]))]
        print("| %s | %d | %s | %s | %s | %s | %s | %s | %s | %s | %s | %s | %s | %s | %s | %s |" % (
            lab, len(rs), fmt(mean(rs, lambda r: r["cwnd_sim"]), 0), fmt(mean(rs, lambda r: r["share_any"]), 4), fmt(mean(rs, lambda r: r["share_by_0p4"]), 4),
            fmt(mean(rs, lambda r: r["dh_p50"])), " ; ".join(wd) or "-", " ; ".join(wo) or "-",
            fmt(mean(rs, lambda r: r["rev4"]["spare_bind"])), fmt(mean(rs, lambda r: r["rev4"]["n_tau_lt_T"]), 0), fmt(mean(rs, lambda r: r["rev4"]["collapse_t"])),
            fmt(mean(rs, lambda r: r["rev4"]["floor_bind"])), fmt(mean(rs, lambda r: r["g"]["repair_sends"] + r["g"]["rep_sent"]), 0),
            fmt(mean(rs, lambda r: r["g"]["queue_drop"]), 0), fmt(mean(rs, lambda r: r["g"]["fq_evict"]), 0), fmt(mean(rs, lambda r: r["recovery_time"]), 2)))


if __name__ == "__main__":
    main()
