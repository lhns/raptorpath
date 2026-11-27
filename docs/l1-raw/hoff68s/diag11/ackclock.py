#!/usr/bin/env python3
"""beta quantiles against the ack clock N*xdot*L/s (xdot = d qab / dt from [QCC]); room (W-F)/s. Usage: ackclock.py <dir>"""
import sys, os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import join68r as J, series as SE
from collections import defaultdict
N = 1.25
q = lambda v, p: sorted(v)[int(p * (len(v) - 1))] if v else float("nan")
def rows(d, arms="G"):
    for arm, rep, fn in J.rows(d):
        if arm[0] not in arms or arm == "GBU": continue
        Q = SE.qcc(fn)
        ws = [w for w in J.windows(fn) if w[1] == "on"]
        out = []
        for j in range(1, len(ws)):
            t, t0 = ws[j][0], ws[j-1][0]
            for pid in (0, 1):
                a = Q.get((t, pid)); b = Q.get((t0, pid))
                if not a or not b or pid not in ws[j][2]: continue
                p = ws[j][2][pid]
                if not p.get("R") or not p.get("beta") or not p.get("beta_q"): continue
                s = p["R"] * p["L"] / 1e3 / p["beta"]
                xd = (int(a["qab"]) - int(b["qab"])) / (t - t0)
                if xd <= 0: continue
                ack = N * xd * p["L"] / 1e3 / s
                room = max(p["win"] - p["F"], 0) / s
                out.append(dict(arm=arm, rep=rep, pid=pid, t=t, j=j, s=s, xd=xd, ack=ack, room=room, p=p, ws=ws))
        yield arm, rep, fn, out
if __name__ == "__main__":
    A = defaultdict(lambda: defaultdict(list))
    for arm, rep, fn, out in rows(sys.argv[1]):
        for r in out:
            g = f"{arm} p{r['pid']}"; p = r["p"]; bq = p["beta_q"]
            k = "startup" if r["t"] < 2 else "run"
            A[g + " " + k]["c50"].append(bq[1] / r["ack"]); A[g + " " + k]["c95"].append(bq[2] / r["ack"])
            A[g + " " + k]["room"].append(r["room"]); A[g + " " + k]["pacer_vs_x"].append(p["R"] / N / r["xd"])
            A[g + " " + k]["fix_vs_now_end"].append(min(p["beta"], r["room"] + r["ack"]) / p["beta"])
    for g in sorted(A):
        a = A[g]
        print(f"{g}: n={len(a['c50'])} bq50/ack p10/50/90={q(a['c50'],.1):.2f}/{q(a['c50'],.5):.2f}/{q(a['c50'],.9):.2f}  bq95/ack p50/90/max={q(a['c95'],.5):.2f}/{q(a['c95'],.9):.2f}/{max(a['c95']):.2f}"
              f"  (W/srtt)/xdot p50/90={q(a['pacer_vs_x'],.5):.2f}/{q(a['pacer_vs_x'],.9):.2f}  room p50/90={q(a['room'],.5):.0f}/{q(a['room'],.9):.0f}  fix/now(end) p10/50={q(a['fix_vs_now_end'],.1):.2f}/{q(a['fix_vs_now_end'],.5):.2f}")
