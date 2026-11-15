# §48b registered cost attribution (plan W1.1, fixed before the result; reported only).
import json, os, re, sys, statistics as st
D = sys.argv[1]
LINK = {"c2-100": {0: 12.5e6}, "c8-100": {0: 12.5e6, 1: 2.5e6}}   # bytes/s: c2 100 Mbit, c3 20 Mbit
B_MIN = 98
rows = {}
for ln in open(os.path.join(D, "run/bbr48/ld.log"), encoding="utf-8", errors="replace"):
    if ln.startswith("P38ROW "):
        r = json.loads(ln[7:])
        rows[(r["cell"], r["arm"], str(r["seed"]), int(r["rep"]))] = r["mbps"]
T = re.compile(r"\] t=([0-9.]+)s")
def series(path):
    diag, qcc = [], []
    for ln in open(path, encoding="utf-8", errors="replace"):
        m = T.search(ln)
        if not m: continue
        t = float(m.group(1))
        if "[QCC]" in ln:
            qcc.append((t, {int(a): (b, int(c)) for a, b, c in re.findall(r"p(\d+):qm=(\S) qrt_us=\d+ qsr_us=\d+ qbw=(\d+)", ln)}))
        elif "[DIAG]" in ln:
            pm = re.search(r"\bpaused=(\d+)%", ln)
            dg = {int(a): int(b) - int(c) for a, b, c in re.findall(r"dgq(\d+)\[hand=(\d+) tx=(\d+)", ln)}
            sent = {int(a): int(b) for a, _, b in re.findall(r"p(\d+):infl=.*?qlp=(\d+)/(\d+)", ln)}
            sent = {int(a): int(b) for a, _l, b in re.findall(r"\bp(\d+):infl=[^|]*?qlp=(\d+)/(\d+)", ln)}
            diag.append((t, int(pm.group(1)) if pm else None, dg, sent))
    return diag, qcc
out = []
for cell in ("c2-100", "c8-100"):
    base_med = st.median([v for (c, a, s, r), v in rows.items() if c == cell and a == "BASE"])
    res = []
    for (c, a, s, r), mbps in sorted(rows.items()):
        if c != cell or a != "CAND": continue
        f = os.path.join(D, "attr", f"{cell}@def-CAND-s{s}-r{r}-a1-c.log")
        diag, qcc = series(f)
        cnt = {"STORE": 0, "QUINN": 0, "PROBERTT": 0, "UNDERREAD": 0, "OTHER": 0}; n = 0
        prev = None
        for t, paused, dg, sent in diag:
            if t < 1.0:
                prev = sent; continue
            q = min(qcc, key=lambda x: abs(x[0] - t))[1] if qcc else {}
            for leg in LINK[cell]:
                if prev is None or sent.get(leg) is None or prev.get(leg) is None or sent[leg] <= prev[leg]:
                    continue          # active leg: qlp sent advanced
                n += 1
                B = dg.get(leg); qm, qbw = q.get(leg, (None, None))
                if paused is not None and paused >= 50 and B is not None and B < B_MIN: cnt["STORE"] += 1
                elif B is not None and B >= B_MIN and qm in ("B", "S", "D"): cnt["QUINN"] += 1
                elif qm == "R": cnt["PROBERTT"] += 1
                elif qbw is not None and qbw < 0.8 * LINK[cell][leg]: cnt["UNDERREAD"] += 1
                else: cnt["OTHER"] += 1
            prev = sent
        sh = {k: v / n for k, v in cnt.items()} if n else {}
        res.append((mbps, s, r, n, sh, 100 * (mbps / base_med - 1)))
    res.sort()
    print(f"== {cell}  BASE median {base_med:.2f} Mbit/s; CAND rows sorted by goodput (deficit vs BASE median); class line shares over active legs at t>=1 s")
    for mbps, s, r, n, sh, d in res:
        print(f"  s{s} r{r} {mbps:7.2f} ({d:+5.2f}%) n={n:4d} " + " ".join(f"{k}={v:.3f}" for k, v in sh.items()))
    low = res[: max(1, len(res) // 3)]
    agg = {k: st.mean(x[4][k] for x in low) for k in low[0][4]}
    top = max(agg, key=agg.get)
    print(f"  lowest third (n={len(low)}): mean shares " + " ".join(f"{k}={v:.3f}" for k, v in agg.items()) + f"  -> names the cost: {top}")
