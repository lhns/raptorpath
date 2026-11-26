"""Table of out3e/rev4/floor2.jsonl: per cell and build, the three variants (off, evidenced 10.3, new = evidenced_overdue + elig rack)."""
import json
import os

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
FILE = os.path.join(HERE, "out3e", "rev4", "floor2.jsonl")
NF = json.load(open(os.path.join(HERE, "nf_stage3e.json")))["by_n"]["3"]["cells"]


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
    R = {}
    for ln in open(FILE, encoding="utf-8"):
        if ln.strip():
            d = json.loads(ln)
            R.setdefault(d["label"], []).append(d)
    print("| cell | build | variant | floors | repeats by knowledge / timeout | resends (dup) | elig_R_bind | delivered | on time | p50 | recovery | link slots first / resends / repairs / floors | viol (carrier, elig) | all finished |")
    print("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|")
    for lab in sorted(R):
        rs = R[lab]
        cell = lab.split(" | ")[0]
        rest = lab.split(" | ")[1]
        build = rest.split(" ")[-2] if " v=" in rest else ""
        var = rest.split("v=")[-1]
        g = lambda k: m(rs, lambda r, k=k: r["g"].get(k) or 0)                  # noqa: E731
        eb = m(rs, lambda r: r["rev4"]["elig_R_bind"] / max(r["rev4"]["elig_n"], 1)) if rs[0].get("rev4") else float("nan")
        print("| %s | %s | %s | %.0f | %.0f / %.0f | %.0f (%.0f) | %.3f | %.4f | %.4f | %.3f | %s | %.0f / %.0f / %.0f / %.0f | %.0f, %.0f | %s |" % (
            cell, rest.split(" ")[2] + " " + rest.split(" ")[3] + " " + build, var, g("floors"), g("rep_fired_by_knowledge"), g("rep_fired_by_timeout"), g("repair_sends"), g("dup"), eb,
            m(rs, lambda r: r["share_any"]), m(rs, lambda r: r["share_by_0p4"]), m(rs, lambda r: r["dh_p50"]),
            "%.2f" % m(rs, lambda r: r["recovery_time"]) if rs[0].get("recovery_time") is not None else "-",
            m(rs, lambda r: r["n_sent"]), g("repair_sends"), g("rep_sent"), g("floors"), g("rep_fire_viol_carrier"), m(rs, lambda r: r["rev4"]["elig_viol"]), all(r["finished"] for r in rs)))


if __name__ == "__main__":
    main()
