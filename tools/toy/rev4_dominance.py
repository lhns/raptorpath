"""The registered dominance reading (three-knobs-revision4.md 3.1) for the overload cells whose lifetime sweep collapses: the finite-T rows of Revision 4 (both parts)
pooled into one point per seed, compared with each baseline by paired differences, delivered and on time not worse beyond the bar, delay p50 not worse.  The rule is
read as registered (proof_s1.read_good against the bar max(t_bar, A/A floor)); reported only.

    python rev4_dominance.py
"""
import json
import os
import sys

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import batch3e as B  # noqa: E402
from proof_s1 import read_good, t_bar  # noqa: E402

FILE = os.path.join(B.OUT, "rev4", "step2.jsonl")
NF = json.load(open(os.path.join(HERE, "nf_stage3e.json")))
KEYS = (("share_any", +1.0, "delivered"), ("share_by_0p4", +1.0, "on time 0.4 s"), ("dh_p50", -1.0, "delay p50"))


def rows():
    d = {}
    for ln in open(FILE, encoding="utf-8"):
        if ln.strip():
            r = json.loads(ln)
            d.setdefault(r["label"], {})[r["seed"]] = r
    return d


def nf(cell, k):
    return float(NF["by_n"]["3"]["cells"].get(cell, {}).get(k, 0.0))


def read(new, base, cell, units, seeds):
    out = []
    for k, sign, nm in KEYS:
        x = np.asarray([new[s][k] for s in seeds])
        y = np.asarray([base[s][k] for s in seeds])
        g = sign * (x - y)
        ref = float(y.mean())
        m = max(t_bar("share_by_0p4" if sign > 0 else "recovery_time", ref, units), nf(cell, k))
        rd = read_good(g, m)
        out.append((nm, float(x.mean()), float(y.mean()), rd["word"], rd["mean"], rd["m"]))
    return out


def baseline_old(cell_label, design, seeds):
    """Stored BASE rows (today's Bulk C3, ADR-0084's C1) of OV, delta = 50, as run in stage 3e."""
    B.build()
    fps = [j for j in B.PARTS["BASE"] if j.cell == cell_label and j.design == design]
    res = []
    for j in fps:
        r = {s: B.load(j.fp, s) for s in seeds}
        if all(v is not None for v in r.values()):
            res.append((j.arm or j.key, r))
    return res


def main():
    R = rows()
    seeds = (0, 1, 2)
    for cell, base_cell, units, finite, rts in (
        ("OV", "OV", 120000, [("OV-2x2 | OV | new RL T=0p4 d=50 clamp=1 wait=1 eq=0 cwnd=cell", "OV-2x2 | OV | new RL T=2 d=50 clamp=1 wait=1 eq=0 cwnd=cell")],
         [("today's Realtime, as stored, clamp on", "RT-arms | OV | rt RL T=- d=50 clamp=1 wait=0 eq=0 cwnd=1000.0"),
          ("today's Realtime, engine queue, clamp on", "RT-arms | OV | rt RL T=- d=50 clamp=1 wait=0 eq=1 cwnd=1000.0")]),
        ("OV-end", "OV", 152000, [("OV-end | OV-end | new RL T=0p4 d=50 clamp=1 wait=1 eq=0 cwnd=cell", "OV-end | OV-end | new RL T=2 d=0.5 clamp=1 wait=1 eq=0 cwnd=cell")],
         [("today's Realtime, as stored, clamp on", "RT-arms | OV-end | rt RL T=- d=50 clamp=1 wait=0 eq=0 cwnd=1000.0"),
          ("today's Realtime, engine queue, clamp on", "RT-arms | OV-end | rt RL T=- d=50 clamp=1 wait=0 eq=1 cwnd=1000.0"),
          ("Revision 3 with the clamp, T = 0.4", "OV-end | OV-end | new RL T=0p4 d=50 clamp=1 wait=0 eq=0 cwnd=cell")]),
    ):
        labs = finite[0]
        pooled = {s: {k: float(np.mean([R[l][s][k] for l in labs])) for k, _, _ in KEYS} for s in seeds}
        print("\n### %s: Revision 4 (both parts), the finite-T rows pooled: %s" % (cell, " + ".join(l.split(" | ")[2] for l in labs)))
        print("| baseline | readout | Revision 4 | baseline | reading (paired, bar) |\n|---|---|---|---|---|")
        bases = [(nm, {s: R[l][s] for s in seeds}) for nm, l in rts]
        if cell == "OV":
            bases += [("stored " + nm, {s: dict(v) for s, v in r.items()}) for nm, r in baseline_old("OV", "adr", seeds)[:1]]
            bases += [("stored " + nm, {s: dict(v) for s, v in r.items()}) for nm, r in baseline_old("OV", "bulk", seeds)[:1]]
        for nm, b in bases:
            for rd in read(pooled, b, base_cell, units, seeds):
                print("| %s | %s | %.4f | %.4f | %s (mean diff %+.4f, bar %.4f) |" % (nm, rd[0], rd[1], rd[2], rd[3], rd[4], rd[5]))


if __name__ == "__main__":
    main()
