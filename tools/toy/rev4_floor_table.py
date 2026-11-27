"""Scores the floor-repeat rows against three-knobs-revision4.md 10.5 (bands registered before any run).  Reported only."""
import json
import os

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
FILE = os.path.join(HERE, "out3e", "rev4", "floor.jsonl")
NF = json.load(open(os.path.join(HERE, "nf_stage3e.json")))["by_n"]["3"]["cells"]


def load():
    R = {}
    for ln in open(FILE, encoding="utf-8"):
        if ln.strip():
            d = json.loads(ln)
            R.setdefault(d["label"], []).append(d)
    return R


def m(rs, f):
    v = []
    for r in rs:
        try:
            x = f(r)
        except (KeyError, TypeError):
            x = None
        if x is not None and x == x:
            v.append(x)
    return float(np.mean(v)) if v else float("nan")


def main():
    R = load()
    print("| cell | build | floors off | floors on | suppressed | by timeout | by knowledge | floor share off / on | delivered off / on | on time off / on | p50 off / on | recovery off / on | viol | finished | vs the A/A floor |")
    print("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|")
    keys = sorted({k.rsplit(" fr=", 1)[0] for k in R})
    for k in keys:
        off, on = R.get(k + " fr=off"), R.get(k + " fr=evidenced")
        if not off or not on:
            continue
        cell = k.split(" | ")[0]
        slots = lambda r: r["g"]["floors"] + r["n_sent"] + r["g"]["repair_sends"] + r["g"]["rep_sent"]        # noqa: E731
        sh = lambda rs: m(rs, lambda r: r["g"]["floors"] / max(slots(r), 1))                                  # noqa: E731
        d_sh = m(on, lambda r: r["share_any"]) - m(off, lambda r: r["share_any"])
        d_ot = m(on, lambda r: r["share_by_0p4"]) - m(off, lambda r: r["share_by_0p4"])
        d_rec = m(on, lambda r: r["recovery_time"]) - m(off, lambda r: r["recovery_time"])
        nfc = NF.get(cell.split(" ")[0], {})
        bar_sh, bar_ot = nfc.get("share_any", float("nan")), nfc.get("share_by_0p4", float("nan"))
        verdict = []
        verdict.append("delivered %+.4f %s" % (d_sh, "in" if abs(d_sh) <= bar_sh else ("out" if bar_sh == bar_sh else "no bar")))
        verdict.append("on time %+.4f %s" % (d_ot, "in" if abs(d_ot) <= bar_ot else ("out" if bar_ot == bar_ot else "no bar")))
        if d_rec == d_rec:
            verdict.append("recovery %+.2f %s" % (d_rec, "in" if abs(d_rec) <= 0.1 else "out"))
        print("| %s | %s | %.0f | %.0f | %.0f | %.0f | %.0f | %.3f / %.3f | %.4f / %.4f | %.4f / %.4f | %.3f / %.3f | %s / %s | %.0f | %s | %s |" % (
            k.split(" | ")[0], k.split(" | ")[1], m(off, lambda r: r["g"]["floors"]), m(on, lambda r: r["g"]["floors"]), m(on, lambda r: r["g"]["rep_suppressed"]),
            m(on, lambda r: r["g"]["rep_fired_by_timeout"]), m(on, lambda r: r["g"]["rep_fired_by_knowledge"]), sh(off), sh(on),
            m(off, lambda r: r["share_any"]), m(on, lambda r: r["share_any"]), m(off, lambda r: r["share_by_0p4"]), m(on, lambda r: r["share_by_0p4"]),
            m(off, lambda r: r["dh_p50"]), m(on, lambda r: r["dh_p50"]),
            "%.2f" % m(off, lambda r: r["recovery_time"]) if off[0].get("recovery_time") is not None else "-", "%.2f" % m(on, lambda r: r["recovery_time"]) if on[0].get("recovery_time") is not None else "-",
            m(on, lambda r: r["g"]["rep_supp_viol"]), all(r["finished"] for r in on), "; ".join(verdict)))


if __name__ == "__main__":
    main()
