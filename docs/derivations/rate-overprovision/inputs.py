"""Per-arm medians of the rate law's live inputs and outputs (source phase).

usage: python -I inputs.py <cell.json>
"""
import json, sys, statistics as st

ORDER = ["HB", "BM", "BP", "I1", "I2", "I3", "AM", "HA", "NA", "AP", "I4", "RM", "HR", "RP"]


def worst(paths):
    # channel_set.rs:40 max_by(loss_rate): ties go to the LAST maximum
    best = None
    for p in paths:
        if best is None or p["pl"] >= best["pl"]:
            best = p
    return best


def med(x):
    return st.median(x) if x else float("nan")


def row_stats(r):
    ticks = [d for d in r["diag"] if d["src"] > 1000 and d["paths"]]
    w = [worst(d["paths"]) for d in ticks]
    spans = [s for s in r["span"] if s["wsize"] and s["wsize"] > 1]
    rr = [s["rr"] for s in spans]
    out = dict(
        n=len(ticks),
        frac_p1=sum(1 for x in w if x["id"] == 1) / max(1, len(w)),
        plu_w=med([x["plu"] for x in w]),
        rtt_w=med([x["rtt"] for x in w]),
        plu0=med([d["paths"][0]["plu"] for d in ticks]),
        plu1=med([d["paths"][1]["plu"] for d in ticks if len(d["paths"]) > 1]),
        rtt0=med([d["paths"][0]["rtt"] for d in ticks]),
        rtt1=med([d["paths"][1]["rtt"] for d in ticks if len(d["paths"]) > 1]),
        plc0=r["diag"][-1]["paths"][0]["plc"],
        rr_mean=sum(rr) / max(1, len(rr)),
        rr_med=med(rr),
        rr_max=max(rr) if rr else 0,
        rr_at_cap=sum(1 for x in rr if x >= 0.4995) / max(1, len(rr)),
        astar=med([s["astar"] for s in spans if s["astar"]]),
        wsize=med([s["wsize"] for s in spans]),
        cons=(r["cod"] + r["taper"]) / r["src"],
        cod=r["cod"] / r["src"],
        taper=r["taper"] / r["src"],
        retx=r["retx"] / r["src"],
        mbps=r["mbps"],
        loss=[t["loss"] for t in r["truth"] if "loss" in t],
    )
    return out


def main(path):
    rows = json.load(open(path))
    by = {}
    for r in rows:
        by.setdefault(r["arm"], []).append(row_stats(r))
    keys = ["frac_p1", "plu_w", "rtt_w", "plu0", "plu1", "rtt0", "rtt1", "rr_mean", "rr_med", "rr_max",
            "rr_at_cap", "astar", "cons", "cod", "taper", "retx", "mbps"]
    print("arm  n " + " ".join(f"{k:>8}" for k in keys))
    for a in ORDER:
        if a not in by:
            continue
        v = by[a]
        print(f"{a:3} {len(v):2} " + " ".join(f"{med([x[k] for x in v]):8.4f}" for k in keys))
    allloss = [l for a in by for x in by[a] for l in [x["loss"]]]
    print("truth loss per leg (median over rows):", [round(med([l[i] for l in allloss]), 4) for i in range(len(allloss[0]))])


if __name__ == "__main__":
    main(sys.argv[1])
