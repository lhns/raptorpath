#!/usr/bin/env python3
"""Spike windows (beta_q p95 / p50) against ProbeRtt steps, qm, L, and srtt/m. Usage: spikes.py <diag-ld dir>"""
import sys, os, statistics as st
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import join68r as J, series as SE, probertt68r as PR

def main(d):
    for arm, rep, fn in J.rows(d):
        if arm[0] != "G" or arm == "GBU": continue
        Q = SE.qcc(fn); steps = PR.steps(fn)
        ws = [w for w in J.windows(fn) if w[1] == "on"]
        for pid in (0, 1):
            cs = []
            for j in range(len(ws)):
                if ws[j][0] < 2.0 or pid not in ws[j][2]: continue
                c = J.classify(ws, j, pid)
                if not c: continue
                p = ws[j][2][pid]
                q = Q.get((c["t"], pid), {})
                c["srtt"] = 1.25 * p["win"] / p["R"] if p.get("R") else None
                c["m"] = float(q.get("qrt_us", 0) or 0) / 1e6
                c["qm"] = q.get("qm", "?")
                c["ratio"] = c["bq95"] / c["bq50"] if c["bq50"] else 0
                c["near"] = any(-0.3 <= c["t"] - s <= 1.0 for s in steps)
                c["exc"] = max(c["E"] - c["batch"], 0)
                cs.append(c)
            if not cs: continue
            sr = st.median([c["srtt"] for c in cs if c["srtt"]]); mm = st.median([c["m"] for c in cs if c["m"]] or [0])
            over = [c for c in cs if c["E"] > c["batch"]]
            tot = sum(c["exc"] for c in cs)
            near_exc = sum(c["exc"] for c in cs if c["near"])
            big = [c for c in cs if c["ratio"] > 1.5]
            rmax = max(c["ratio"] for c in cs)
            # excess share by ratio band
            bands = [(0, 1.2), (1.2, 1.5), (1.5, 2.0), (2.0, 99)]
            bs = " ".join(f"[{a},{b}):{sum(c['exc'] for c in cs if a <= c['ratio'] < b)/max(tot,1):.2f}/n{sum(1 for c in cs if a <= c['ratio'] < b)}" for a, b in bands)
            print(f"{arm} r{rep} p{pid}: win={len(cs)} over={len(over)} srtt_med={sr*1e3:.1f}ms m={mm*1e3:.1f}ms srtt/m={sr/mm if mm else 0:.2f} ratio_max={rmax:.2f} "
                  f"ratio>1.5: n={len(big)} near_probertt={sum(c['near'] for c in big)} | excess near ProbeRtt={near_exc/max(tot,1):.2f} | excess by ratio band {bs}")

if __name__ == "__main__":
    main(sys.argv[1])
