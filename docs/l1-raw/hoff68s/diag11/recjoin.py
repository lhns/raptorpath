#!/usr/bin/env python3
"""Join HOFF windows with DIAG qce (quinn congestion events), qlp, F/win, and L terms. Usage: recjoin.py <dir>"""
import re, sys, os, statistics as st
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import join68r as J
from collections import defaultdict

def diag(fn):
    out = {}
    with open(fn, "rb") as f:
        for raw in f:
            ln = re.sub(r"\x1b\[[0-9;]*m", "", raw.decode("utf-8", "replace"))
            if ln.startswith("[DIAG]"):
                t = float(re.search(r"\bt=([\d.]+)s", ln).group(1))
                for pid, ce, lo, se in re.findall(r"\bp(\d+):infl=.*?qce=(\d+) qlp=(\d+)/(\d+)", ln):
                    out[(t, int(pid))] = (int(ce), int(lo), int(se))
    return out

def hoff_terms(fn):
    out = {}
    with open(fn, "rb") as f:
        for raw in f:
            ln = re.sub(r"\x1b\[[0-9;]*m", "", raw.decode("utf-8", "replace"))
            if "[HOFF]" in ln and "gate=on" in ln:
                t = float(re.search(r"\bt=([\d.]+)s", ln).group(1))
                for pid, body in re.findall(r"\bp(\d+)\[([^\]]*)\]", ln):
                    d = {}
                    for k in ("tau_o", "tau_p", "l_o", "l_p"):
                        m = re.search(rf"\b{k}=([\d.]+)ms", body)
                        d[k] = float(m.group(1)) if m else None
                    out[(t, int(pid))] = d
    return out

def main(d):
    agg = defaultdict(lambda: defaultdict(list))
    for arm, rep, fn in J.rows(d):
        if arm[0] != "G" or arm == "GBU": continue
        D = diag(fn); T = hoff_terms(fn)
        ws = [w for w in J.windows(fn) if w[1] == "on"]
        for j in range(1, len(ws)):
            t = ws[j][0]
            if t < 2: continue
            for pid in (0, 1):
                if pid not in ws[j][2] or pid not in ws[j-1][2]: continue
                c = J.classify(ws, j, pid)
                if not c: continue
                d1, d0 = D.get((t, pid)), D.get((ws[j-1][0], pid))
                if not d1 or not d0: continue
                dce = d1[0] - d0[0]; dlo = d1[1] - d0[1]
                p = ws[j][2][pid]
                r = c["bq95"] / c["bq50"]
                exc = max(c["E"] - c["batch"], 0)
                t1, t0 = T.get((t, pid)), T.get((ws[j-1][0], pid))
                mv = None
                if t1 and t0:
                    mv = max(("tau_o", "tau_p", "l_o", "l_p"), key=lambda k: abs((t1[k] or 0) - (t0[k] or 0)))
                    dl = abs((t1[mv] or 0) - (t0[mv] or 0))
                    if dl < 0.1: mv = "none"
                agg[f"{arm} p{pid}"]["rows"].append((r, exc, dce, dlo, p["F"] / p["win"] if p.get("win") else 1, mv))
    for g in sorted(agg):
        v = agg[g]["rows"]
        tot = sum(x[1] for x in v) or 1
        spk = [x for x in v if x[0] > 1.5]; calm = [x for x in v if x[0] < 1.2]
        md = lambda a: st.median(a) if a else float("nan")
        # correlation-ish: excess share in windows with dce above row median
        mce = md([x[2] for x in v])
        hi = [x for x in v if x[2] > mce]
        print(f"{g}: n={len(v)} dce/window med={mce} | spikes n={len(spk)} dce med={md([x[2] for x in spk])} F/win med={md([x[4] for x in spk]):.2f} | calm dce med={md([x[2] for x in calm])} F/win={md([x[4] for x in calm]):.2f}"
              f" | excess in dce>med windows={sum(x[1] for x in hi)/tot:.2f} ({len(hi)}/{len(v)}) | windows with zero dce={sum(1 for x in v if x[2]==0)}")
        mvc = defaultdict(lambda: [0, 0.0])
        for x in v:
            mvc[x[5]][0] += 1; mvc[x[5]][1] += x[1]
        print("    L term that moved most:", {k: (n, round(e / tot, 2)) for k, (n, e) in mvc.items()})

if __name__ == "__main__":
    main(sys.argv[1])
