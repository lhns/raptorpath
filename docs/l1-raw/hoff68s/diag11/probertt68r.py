#!/usr/bin/env python3
"""Do the over-batch / worst / reopen-miss windows line up with ProbeRtt? Usage: probertt68r.py <diag-ld dir>
bbr_rs rows: ProbeRtt entries = [QCC] lines where qrn steps (path 0's qrn; the window is 'near' if 0 <= t - t_step_line <= 1.0 s
or the step line is within one [HOFF] window after it). Quinn-BBR rows (Q) have no [QCC]: proxy = t mod 10 in [0, 1.0]."""
import re
import sys
from collections import defaultdict
sys.path.insert(0, __import__("os").path.dirname(__file__))
import join68r as J  # noqa: E402


def steps(fn):
    out, last = [], {}
    for t, q in J.qcc_steps(fn):
        for pid, n in q.items():
            if pid in last and n > last[pid]:
                out.append(t)
            last[pid] = n
    return sorted(set(out))


def main(d):
    agg = defaultdict(lambda: defaultdict(lambda: [0, 0]))
    for arm, rep, fn in J.rows(d):
        if arm[0] not in "GQ" or arm in ("GLR",):
            continue
        st = steps(fn)
        ws = [w for w in J.windows(fn) if w[1] == "on"]
        for j, (t, gate, P) in enumerate(ws):
            if t < 2.0:
                continue
            if st:
                near = any(-0.3 <= t - s <= 1.0 for s in st)
            else:
                near = (t % 10.0) <= 1.0
            for pid in P:
                c = J.classify(ws, j, pid)
                if not c:
                    continue
                grp = f"{arm} p{pid}"
                over = c["E"] > c["batch"]
                miss = c["reo"] is not None and c["L"] and c["reo"] > c["L"]
                for k, v in (("all", True), ("over", over), ("miss", miss), ("over>50", c["E"] > 50)):
                    if v:
                        agg[grp][k][0] += 1
                        agg[grp][k][1] += near
        if st:
            print(f"{arm} r{rep} probertt steps at {st}")
    for grp in sorted(agg):
        a = agg[grp]
        print(grp, " ".join(f"{k}: {a[k][1]}/{a[k][0]} near" for k in ("all", "over", "over>50", "miss")))


if __name__ == "__main__":
    main(sys.argv[1])
