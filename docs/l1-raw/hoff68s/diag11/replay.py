#!/usr/bin/env python3
"""Counterfactual replay of beta_fix = min(beta, room + N*xdot*L/s) on the stored per-window series.
Model (stated in reg-handoff-gate.md section 11.4): within a window the cap is cap_w = room_end + N*xdot_w*L_end/s
(room and L from the end sample, xdot the window's mean ack rate); B under the fix is admitted up to the higher of this
and the previous window's cap, so B_fix p95 = min(B p95, max(cap_w, cap_{w-1}) + batch), B_fix max likewise;
E_fix = B_fix p95 - beta_fix_end, beta_fix_end = min(beta_end, cap_w). Usage: replay.py <dir> [arms]"""
import sys, os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import ackclock as AC
from collections import defaultdict
q = lambda v, p: sorted(v)[int(p * (len(v) - 1))] if v else float("nan")
arms = sys.argv[2] if len(sys.argv) > 2 else "G"
A = defaultdict(lambda: defaultdict(list))
for arm, rep, fn, out in AC.rows(sys.argv[1], arms):
    prev = {}
    for r in out:
        p = r["p"]; pid = r["pid"]
        cap = r["room"] + r["ack"]
        capp = prev.get(pid, cap); prev[pid] = cap
        B95, Bmax = p["B"][1], p["B"][2]; batch = p["batch"]; beta = p["beta"]; bq = p["beta_q"]
        E = B95 - beta
        bfe = min(beta, cap)
        B95f = min(B95, max(cap, capp) + batch)
        Ef = B95f - bfe
        g = f"{arm} p{pid}" + (" t<2" if r["t"] < 2 else "")
        a = A[g]
        a["E"].append(E); a["Ef"].append(Ef); a["over"].append(E > batch); a["overf"].append(Ef > batch)
        a["bind"].append(cap < bq[2]); a["bind50"].append(cap < bq[1])
        a["cov"].append(bfe / (r["xd"] * p["L"] / 1e3 / r["s"]))
        a["Bmax"].append(Bmax); a["Bmaxf"].append(min(Bmax, max(cap, capp) + batch))
        a["bq95"].append(bq[2] / bq[1]); a["bq95f"].append(min(bq[2], cap) / min(bq[1], cap))
for g in sorted(A):
    a = A[g]; n = len(a["E"])
    print(f"{g}: n={n} E p50/p90/max now={q(a['E'],.5):.0f}/{q(a['E'],.9):.0f}/{max(a['E']):.0f} fix={q(a['Ef'],.5):.0f}/{q(a['Ef'],.9):.0f}/{max(a['Ef']):.0f}"
          f" | over-batch now={sum(a['over'])} fix={sum(a['overf'])} | Bmax p90 now/fix={q(a['Bmax'],.9):.0f}/{q(a['Bmaxf'],.9):.0f}"
          f" | spread p95/p50 p90 now={q(a['bq95'],.9):.2f} fix={q(a['bq95f'],.9):.2f} | cap binds below beta p95 in {sum(a['bind'])/n:.2f}, below p50 in {sum(a['bind50'])/n:.2f}"
          f" | beta_fix_end/(xdot L/s) p5/p50={q(a['cov'],.05):.2f}/{q(a['cov'],.5):.2f}")
