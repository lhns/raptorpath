"""Tick-level law evaluation: each [SPAN] rr against the law at the nearest
[DIAG] tick's worst-path plu and rtt (time mean over the Auto-preset arms and
R+-), for several sigma2. Shows whether the plu fluctuation, not a large
sigma2, carries the time-mean r.

usage: python -I tick.py <cell.json> <tput bytes/s> <sigma2,...>
"""
import bisect, json, os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))  # law.py beside this script
from law import law

ARMS = {"auto": ("AM", "HA", "NA", "AP"), "rt": ("RM", "RP"), "I3": ("I3",)}


def worst(ps):
    b = None
    for p in ps:
        if b is None or p["pl"] >= b["pl"]:
            b = p
    return b


def main(path, tput, s2s):
    rows = json.load(open(path))
    for grp, arms in ARMS.items():
        obs, n = 0.0, 0
        pred = {s: 0.0 for s in s2s}
        for r in rows:
            if r["arm"] not in arms:
                continue
            d = [x for x in r["diag"] if x["paths"]]
            ts = [x["t"] for x in d]
            for s in r["span"]:
                if not s["wsize"] or s["wsize"] <= 1 or s["t"] < 2.0:
                    continue
                i = min(bisect.bisect_left(ts, s["t"]), len(ts) - 1)
                w = worst(d[i]["paths"])
                if d[i]["src"] <= 1000:
                    continue
                obs += s["rr"]; n += 1
                for s2 in s2s:
                    pred[s2] += law(r["delta"], w["plu"], s2, 201, w["rtt"] / 1e3, tput)[0]
        print(f"{path} {grp}: ticks {n} mean rr {obs/n:.4f} | " +
              " ".join(f"s2={s2:g}: {pred[s2]/n:.4f}" for s2 in s2s))


if __name__ == "__main__":
    main(sys.argv[1], float(sys.argv[2]), [float(x) for x in sys.argv[3].split(",")])
