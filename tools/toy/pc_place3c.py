"""Per-seed positive-control placement (three-knobs.md 15.5, 15.6.3), cell E-I2n.  From each seed's own run of S4 at T = inf, H = inf
(arm C3S), D_u = dlv - h of the dead-tail units:
    p_seed = ceil to 1 ms of ( max D_u + 2 ms );  usable iff  min D_u > 0.99 p_seed + 2 ms  (the whole ramp lies between 0.99 p and p).
Also stores, from the same run, F_seed(T) = |{u : D_u <= T}| / units and from it K-hat and P-hat (15.5 item 5).
Writes pc_stage3c.json once (write-once, like the A/A floor): the literal reading, D_u over every seq >= dead[0].
--dev writes pc_stage3c_dev.json (the deviation, reported with its timing): D_u over the units whose FIRST SEND fell inside the outage window,
because the source clock accumulates float error and the first seq >= dead[0] is sent just before the window (a live unit, age 13 ms), which
makes the literal reading read "ramp too wide" in every seed.  The PC arms of batch3c read the deviation file.   python pc_place3c.py [--dev]"""
import json
import math
import os
import sys

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import summarize3c as S  # noqa: E402

DEV = "--dev" in sys.argv
PATH = os.path.join(HERE, "pc_stage3c_dev.json" if DEV else "pc_stage3c.json")
CELL = "E-I2n"


def F_of(r, T):
    """share_T of the T = inf run read at T: units with age <= T over units handed over (ages <= 0.2 s are all within any T used here)."""
    late = np.asarray(r["late_ages"])
    return (r["delivered"] - int((late > T).sum())) / r["units"]


def khat_phat(r, p):
    F = lambda x: F_of(r, x * p)           # noqa: E731
    K = 2.0 * (F(1.01) - F(0.99)) - (F(1.02) - F(0.98))
    P = 2.0 * (F(1.0) - 0.5 * (F(0.99) + F(1.01))) - (F(1.0) - 0.5 * (F(0.98) + F(1.02)))
    return K, P


def main():
    if os.path.exists(PATH):
        print(PATH, "exists; placement is write-once")
        return
    out = {CELL: {}}
    skipped = []
    for s in S.seeds_of(CELL, "C3S"):
        r = S.R(CELL, "C3S", s)
        D = np.asarray(r["tail_ages"])
        if DEV:
            import batch3c
            import toy2
            cfg = batch3c.s4_cfg(CELL, s, math.inf, math.inf)
            sim = toy2.Sim(cfg)
            sim.run()
            d0, w0, wl = cfg.dead
            D = np.asarray([sim.dlv[u] - sim.h_seq[u] for u in range(d0, sim.n_sent) if w0 <= sim.sent_t[u] < w0 + wl and sim.dlv[u] >= 0])
            skipped_live = sum(1 for u in range(d0, sim.n_sent) if not (w0 <= sim.sent_t[u] < w0 + wl))
            r = dict(r, tail_undelivered=sum(1 for u in range(d0, sim.n_sent) if w0 <= sim.sent_t[u] < w0 + wl and sim.dlv[u] < 0))
            print("seed %d: %d tail units live (first send before the window), %d dead" % (s, skipped_live, len(D)))
        if r["tail_undelivered"] or len(D) == 0:
            skipped.append((s, "tail units undelivered at T = inf"))
            continue
        p = math.ceil((float(D.max()) + 0.002) * 1000.0 - 1e-9) / 1000.0
        if not D.min() > 0.99 * p + 0.002:
            skipped.append((s, "ramp too wide: min %.4f, 0.99p + 2 ms = %.4f, p = %.3f" % (D.min(), 0.99 * p + 0.002, p)))
            continue
        K, P = khat_phat(r, p)
        out[CELL][str(s)] = dict(p=p, dmin=float(D.min()), dmax=float(D.max()), n=len(D), Khat=K, Phat=P)
    out["skipped"] = skipped
    out["usable"] = len(out[CELL])
    json.dump(out, open(PATH, "w"), indent=1)
    print("usable seeds: %d of 12" % out["usable"])
    for s, v in out[CELL].items():
        print(s, v)
    for sk in skipped:
        print("skipped", sk)


if __name__ == "__main__":
    main()
