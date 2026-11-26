"""Revision 4, step 2 (three-knobs-revision4.md 5.4): the sizing run, reported only (no verdict).  The wait limit, the clamp and today's Realtime behind the engine
queue are config fields that default off; this runs the doc's sizing set at 3 seeds on one pool.

    python rev4_step2.py [--workers 9] [--only PREFIX] [--seeds 3]

Writes out3e/rev4/step2.jsonl (one line per run, resumable: a finished (label, seed) is skipped) and prints the table by `python rev4_step2.py --table`.
"""
import argparse
import dataclasses
import json
import multiprocessing as mp
import os
import sys
import time

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import batch3e as B  # noqa: E402
import batch3b  # noqa: E402
import toy2  # noqa: E402

OUT = os.path.join(B.OUT, "rev4")
FILE = os.path.join(OUT, "step2.jsonl")
INF = float("inf")
STEADY = {"OV": [(3.0, 9.0)], "OV-end": [(3.0, 5.5), (9.0, 15.0)], "OV-crit": [(2.0, 9.0)], "K-OV": [(10.0, 55.0)]}


def cfg_of(spec, seed):
    cell = spec["cell"]
    base = spec["base"]
    rp = B.rp(spec["mode"])
    kw = dict(spec.get("kw", {}))
    if spec["design"] == "new":
        cfg = B.new_cfg(base, seed, spec["T"], 0.0, spec["delta"], rp, **kw)
    elif spec["design"] == "rt":
        cfg = B.rt_cfg(base, seed, spec["delta"], rp, **kw)
    else:
        raise ValueError(spec["design"])
    return dataclasses.replace(cfg, rev4_gauge=spec["design"] == "new", clamp=spec.get("clamp", False), wait_limit=spec.get("wait", False),
                               eq=spec.get("eq", False), cwnd_sim=spec.get("cwnd", 0.0))


def specs():
    out = []

    def add(group, cell, base, design, mode, T=None, delta=50.0, clamp=False, wait=False, eq=False, cwnd=0.0, **kw):
        lab = "%s | %s | %s %s T=%s d=%g clamp=%d wait=%d eq=%d cwnd=%s" % (group, cell, design, mode, "-" if T is None else B.TAG(T), delta, clamp, wait, eq, cwnd or "cell")
        out.append(dict(label=lab, group=group, cell=cell, base=base, design=design, mode=mode, T=T, delta=delta, clamp=clamp, wait=wait, eq=eq, cwnd=cwnd, kw=kw))

    # 1. OV, the 2x2 (RL arm: wait limit x clamp, T x delta) plus the repair-off arm with and without the wait limit
    for T in (0.4, 2.0, INF):
        for d in (0.5, 50.0):
            for w in (False, True):
                for cl in (False, True):
                    add("OV-2x2", "OV", "OV", "new", "RL", T, d, cl, w)
        for w in (False, True):
            add("OV-2x2", "OV", "OV", "new", "OFF", T, 50.0, False, w)
    # 2. today's Realtime in OV, OV-end, OV-crit: both arms, clamp on and off, delta 50
    ovend = dict(rate2=8000.0, rate2_t=6.0, n_total=152000)
    ovcrit = dict(rate=9600.0, n_total=96000)
    for cell, kw in (("OV", {}), ("OV-end", ovend), ("OV-crit", ovcrit)):
        for eq in (False, True):
            for cl in (False, True):
                add("RT-arms", cell, "OV", "rt", "RL", None, 50.0, cl, False, eq, **kw)
    # 3. OV-end: Revision 4 with the clamp (T 0.4 at delta 50 and 25, T 2 at delta 0.5 and 0.25) and Revision 3 with the clamp at both T
    for T, ds in ((0.4, (50.0, 25.0)), (2.0, (0.5, 0.25))):
        add("OV-end", "OV-end", "OV", "new", "RL", T, ds[0], True, False, **ovend)
        for d in ds:
            add("OV-end", "OV-end", "OV", "new", "RL", T, d, True, True, **ovend)
    # 4. OV-crit: Revision 3 and Revision 4, both with the clamp, T 0.4, delta 50
    for w in (False, True):
        add("OV-crit", "OV-crit", "OV", "new", "RL", 0.4, 50.0, True, w, **ovcrit)
    # 5. K-OV: Revision 3 and Revision 4 with the clamp, both of today's Realtime arms (clamp on), T 0.4
    for w in (False, True):
        add("K-OV", "K-OV", "K-OV", "new", "RL", 0.4, 50.0, True, w)
    for eq in (False, True):
        add("K-OV", "K-OV", "K-OV", "rt", "RL", None, 50.0, True, False, eq)
    # 6. CL-long: the clamp-on rows at the doc's B (the cell's pause 1 000) and at 2 048, against clamp off and repair off
    for T in (0.4, 2.0, INF):
        add("CL-long", "CL-long", "CL-long", "new", "OFF", T, 50.0)
        add("CL-long", "CL-long", "CL-long", "new", "RL", T, 50.0)
        add("CL-long", "CL-long", "CL-long", "new", "RL", T, 50.0, True)
        add("CL-long", "CL-long", "CL-long", "new", "RL", T, 50.0, True, cwnd=2048.0)
    for T in (0.4, 2.0):
        add("CL-long", "CL-long", "CL-long", "new", "RL", T, 50.0, True, True)                    # the wait limit is inert here (x <= 1)
    return out


def windows(sm, cell):
    """Steady-window shares: units handed over in the window, delivered, and delivered within 0.4 s of hand-over."""
    n = sm.n_sent
    h = np.asarray(sm.h_seq[:n])
    dl = np.asarray(sm.dlv[:n])
    res = []
    for a, b in STEADY.get(cell, []):
        handed = sm.hand_count(b - 1e-9) - sm.hand_count(a - 1e-9)
        m = (h >= a - 1e-9) & (h < b - 1e-9)
        got = m & (dl >= 0)
        ot = got & (dl - h <= 0.4)
        res.append(dict(a=a, b=b, handed=int(handed), delivered=int(got.sum()) / max(handed, 1), on_time=int(ot.sum()) / max(handed, 1)))
    return res


def one(task):
    spec, seed = task
    t0 = time.time()
    B.wait_for_memory(3.0)
    sm = toy2.Sim(cfg_of(spec, seed))
    r = sm.run()
    row = dict(label=spec["label"], group=spec["group"], cell=spec["cell"], seed=seed, wall_s=time.time() - t0)
    row["win"] = windows(sm, spec["cell"])
    r = batch3b.clean(r)
    keep = ("share_any", "share_by_0p4", "dh_p50", "dh_p90", "w_p50", "recovery_time", "recovery_status", "finished", "delivered")
    row.update({k: r.get(k) for k in keep})
    g = r.get("gauges", {})
    row["g"] = {k: g.get(k) for k in ("rep_sent", "repair_sends", "rep_qdrop", "rep_lost", "queue_drop", "first_qdrop", "fq_evict", "fq_max", "clamp_n", "r_eff_over_spare",
                                       "dec_units_qdrop", "intake_expired_deq", "pause_events", "max_O")}
    row["rev4"] = r.get("rev4")
    if row["rev4"]:
        row["rev4"] = {k: v for k, v in row["rev4"].items() if k not in ("spare_bins",)}
    row["cwnd_sim"] = sm.B_sim
    row["r_mean"] = r.get("r_mean")
    row["rep3e"] = {k: v for k, v in (r.get("rep3e") or {}).items() if isinstance(v, (int, float, str, type(None)))}
    return row


def done_keys():
    s = set()
    if os.path.exists(FILE):
        for ln in open(FILE, encoding="utf-8"):
            ln = ln.strip()
            if ln:
                d = json.loads(ln)
                s.add((d["label"], d["seed"]))
    return s


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--workers", type=int, default=9)
    ap.add_argument("--seeds", type=int, default=3)
    ap.add_argument("--only", default="")
    ap.add_argument("--list", action="store_true")
    a = ap.parse_args()
    B.build()
    allspecs = [s for s in specs() if s["label"].startswith(a.only)]
    if a.list:
        for s in allspecs:
            print(s["label"])
        print(len(allspecs), "configurations")
        return
    os.makedirs(OUT, exist_ok=True)
    have = done_keys()
    tasks = [(s, k) for k in range(a.seeds) for s in allspecs if (s["label"], k) not in have]
    heavy = lambda t: -(120 if t[0]["cell"].startswith("CL-long") else 40)       # noqa: E731
    tasks.sort(key=heavy)
    print("%d configurations, %d runs to do" % (len(allspecs), len(tasks)), flush=True)
    with mp.Pool(a.workers) as pool, open(FILE, "a", encoding="utf-8") as f:
        for i, row in enumerate(pool.imap_unordered(one, tasks), 1):
            f.write(json.dumps(row) + "\n")
            f.flush()
            print("[%d/%d] %s seed %d %.0fs" % (i, len(tasks), row["label"], row["seed"], row["wall_s"]), flush=True)
    print("finished-process", flush=True)


if __name__ == "__main__":
    mp.freeze_support()
    main()
