"""Positive-control placement (three-knobs.md 13.7.3): from the 12 runs of S3 (R) at T = inf, H = inf (arm C3R), the delivery ages
dlv - h of the dead-tail units, pooled; p = their median rounded to 1 ms, w = their interquartile range.  Writes pc_stage3b.json
(once; the PC arms of batch3b read it) and prints the case of Q3b-7 for each E cell.   python pc_place.py"""
import json
import os
import sys

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import summarize3b as S  # noqa: E402

PATH = os.path.join(HERE, "pc_stage3b.json")


def main():
    if os.path.exists(PATH):
        print(PATH, "exists; placement is done once")
        return
    out = {}
    for cell in ("E-I2n", "E-I2", "E-L10"):
        ages = []
        undel = 0
        for s in S.seeds_of(cell, "C3R"):
            r = S.R(cell, "C3R", s)
            ages += r["tail_ages"]
            undel += r["tail_undelivered"]
        a = np.asarray(ages)
        p = round(float(np.median(a)), 3)
        q1, q3 = np.percentile(a, [25, 75])
        w = float(q3 - q1)
        out[cell] = dict(p=p, w=w, n=len(a), undelivered=undel, lo=float(a.min()), hi=float(a.max()),
                         case="step" if w < 0.01 * p else "ramp")
        print(cell, out[cell])
    json.dump(out, open(PATH, "w"), indent=1)


if __name__ == "__main__":
    main()
