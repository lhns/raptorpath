#!/usr/bin/env python3
"""beta spread in the [HOFF] windows at a ProbeRtt step (qrn increments) vs the restore bound srtt_before/m. Usage: prtt.py <dir>"""
import sys, os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import join68r as J, series as SE
for arm, rep, fn in J.rows(sys.argv[1]):
    if arm[0] != "G" or arm == "GBU": continue
    Q = SE.qcc(fn)
    ws = [w for w in J.windows(fn) if w[1] == "on"]
    for pid in (0, 1):
        out = []
        for j in range(2, len(ws) - 1):
            t, t0 = ws[j][0], ws[j-1][0]
            a, b = Q.get((t, pid)), Q.get((t0, pid))
            if not a or not b or int(a["qrn"]) <= int(b["qrn"]): continue
            for k in (j, j + 1):
                p = ws[k][2].get(pid); pp = ws[j-2][2].get(pid)
                if not p or not pp or not p.get("beta_q"): continue
                srtt_b = 1.25 * pp["win"] / pp["R"]
                m = int(Q.get((ws[k][0], pid), a)["qrt_us"]) / 1e6
                bq = p["beta_q"]
                out.append(f"t={ws[k][0]:.1f} spread={bq[2]/bq[1]:.2f} bound={srtt_b/m:.2f} p95={bq[2]:.0f} B={p['B'][1]:.0f}/{p['B'][2]:.0f}")
        if out: print(f"{arm} r{rep} p{pid}: " + " | ".join(out))
