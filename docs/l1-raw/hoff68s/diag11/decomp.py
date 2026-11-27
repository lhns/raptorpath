#!/usr/bin/env python3
"""Decompose end-sample beta variation into window, srtt, L. Usage: decomp.py <diag-ld dir>"""
import re, sys, os, math, statistics as st
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import join68r as J
import series as SE

def med(v): return st.median(v) if v else float("nan")

def main(d):
    for arm, rep, fn in J.rows(d):
        if arm[0] != "G": continue
        Q = SE.qcc(fn)
        ws = [w for w in J.windows(fn) if w[1] == "on" and w[0] >= 2.0]
        for pid in (0, 1):
            rs = []
            for t, g, P in ws:
                p = P.get(pid)
                if not p or not p.get("win") or not p.get("R") or not p.get("beta"): continue
                srtt = 1.25 * p["win"] / p["R"]
                q = Q.get((t, pid), {})
                rs.append(dict(t=t, beta=p["beta"], win=p["win"], srtt=srtt, L=p["L"], F=p.get("F", 0), qm=q.get("qm", "?"), qbw=float(q.get("qbw", 0) or 0), m=float(q.get("qrt_us", 0) or 0) / 1e6,
                               bq=p.get("beta_q"), B=p.get("B")))
            if len(rs) < 20 or arm == "GBU": continue
            lb = [math.log(r["beta"]) for r in rs]
            lw = [math.log(r["win"]) for r in rs]
            ls = [math.log(r["srtt"]) for r in rs]
            ll = [math.log(r["L"]) for r in rs]
            vb = st.pvariance(lb)
            # covariance shares: var(lb) = cov(lb,lw) - cov(lb,ls) + cov(lb,ll)
            def cov(a, b):
                ma, mb = st.mean(a), st.mean(b); return sum((x - ma) * (y - mb) for x, y in zip(a, b)) / len(a)
            sw, ss, sl = cov(lb, lw) / vb, -cov(lb, ls) / vb, cov(lb, ll) / vb
            mb_ = med([r["beta"] for r in rs]); mw = med([r["win"] for r in rs]); ms = med([r["srtt"] for r in rs]); mL = med([r["L"] for r in rs])
            spk = [r for r in rs if r["beta"] > 1.5 * mb_]
            # rate overstatement: (win/srtt) / qbw
            ov = [ (r["win"] / r["srtt"]) / r["qbw"] for r in rs if r["qbw"] > 0]
            wf = [r["F"] / r["win"] for r in rs]
            print(f"{arm} r{rep} p{pid}: n={len(rs)} beta med={mb_:.1f} sd(logb)={math.sqrt(vb):.3f} shares win/srtt/L={sw:.2f}/{ss:.2f}/{sl:.2f} corr(lw,ls)={cov(lw,ls)/math.sqrt(st.pvariance(lw)*st.pvariance(ls)):.2f}"
                  f" | (win/srtt)/qbw med={med(ov):.2f} p95={sorted(ov)[int(.95*len(ov))] if ov else 0:.2f} | F/win med={med(wf):.2f} | spikes(end>1.5med)={len(spk)}")
            for r in spk[:6]:
                print(f"     t={r['t']:5.1f} beta={r['beta']:6.1f} win/med={r['win']/mw:.2f} srtt/med={r['srtt']/ms:.2f} L/med={r['L']/mL:.2f} F/win={r['F']/r['win']:.2f} qm={r['qm']} srtt={r['srtt']*1e3:.1f}ms m={r['m']*1e3:.1f}")

if __name__ == "__main__":
    main(sys.argv[1])
