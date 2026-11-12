# POST HOC, not registered. Q2: UNDER-READ (qbw < 0.8 x leg link) at c8 vs c2, goodput-weighted per leg; deficit split; R2a coincidence.
import json, os, re, sys, statistics as st
import numpy as np
D = sys.argv[1]; L = os.path.join(D, "full/diag-ld")
LINK = {"c2-100": [12.5e6], "c8-100": [12.5e6, 2.5e6]}
BDP = {"c2-100": [125000], "c8-100": [125000, 100000]}
T = re.compile(r"\] t=([0-9.]+)s")


def parse(path):
    diag, qcc = [], []
    lines = open(path, encoding="utf-8", errors="replace").read().splitlines()
    wi = max([i for i, l in enumerate(lines) if '"warmup":true' in l] or [-1])
    for i, ln in enumerate(lines):
        m = T.search(ln)
        if not m:
            continue
        t = float(m.group(1))
        if ln.startswith("[QCC] t="):
            qcc.append((t, {int(a): (b, int(c)) for a, b, c in re.findall(r"p(\d+):qm=(\S) qrt_us=\d+ qsr_us=\d+ qbw=(\d+)", ln)}))
        elif ln.startswith("[DIAG] t="):
            pm = re.search(r"\bpaused=(\d+)%", ln); gd = re.search(r"\bgood=([0-9.]+)Mbit", ln)
            trip = re.findall(r"dgq(\d+)\[hand=(\d+) tx=(\d+)", ln)
            dg = {int(a): int(b) - int(c) for a, b, c in trip}
            tx = {int(a): int(c) for a, b, c in trip}
            sent = {int(a): int(b) for a, _l, b in re.findall(r"\bp(\d+):infl=[^|]*?qlp=(\d+)/(\d+)", ln)}
            cw = {}
            for part in re.split(r"(?= p\d+:infl=)", ln)[1:]:
                g_, q_ = re.match(r" p(\d+):", part), re.search(r"\bqcwnd=(\d+)", part)
                if g_ and q_:
                    cw[int(g_.group(1))] = int(q_.group(1))
            diag.append(dict(t=t, post_warm=i > wi, paused=int(pm.group(1)) if pm else None,
                             good=float(gd.group(1)) if gd else None, B=dg, tx=tx, sent=sent, cw=cw))
    return diag, qcc


out = []
def P(s=""):
    out.append(s); print(s)


P("POST HOC / NOT REGISTERED. Q2 on the full logs (s48b/full/diag-ld), client [DIAG]/[QCC], lines t >= 1 s, active leg = qlp sent advanced (attr48 rule)")
data = {}
for cell in ("c2-100", "c8-100"):
    for arm in ("BASE", "CAND"):
        for f in sorted(os.listdir(L)):
            if not (f.startswith(f"{cell}@def-{arm}-") and f.endswith("-c.log")):
                continue
            seed, rep = re.search(r"-s(\d+)-r(\d+)-", f).groups()
            diag, qcc = parse(os.path.join(L, f))
            drv = open(os.path.join(L, f.replace("-c.log", "-drv.out")), errors="replace").read()
            qd = {int(a): (int(b), int(c)) for a, b, c in re.findall(r"QDISC cli(\d): .*? Sent (\d+) bytes (\d+) pkt", drv)}
            secs = float(re.search(r'"median_s":([0-9.]+)', drv).group(1))
            recs = []; prev = None
            for d in diag:
                if d["t"] < 1.0:
                    prev = d; continue
                q = min(qcc, key=lambda x: abs(x[0] - d["t"]))[1] if qcc else {}
                legs = []
                for i in range(len(LINK[cell])):
                    act = prev is not None and d["sent"].get(i) is not None and prev["sent"].get(i) is not None and d["sent"][i] > prev["sent"][i]
                    dtx = (d["tx"].get(i, 0) - prev["tx"].get(i, 0)) if prev else 0
                    qm, qbw = q.get(i, (None, None))
                    legs.append(dict(act=act, dtx=max(dtx, 0), qm=qm, qbw=qbw, cw=d["cw"].get(i), B=d["B"].get(i)))
                recs.append(dict(t=d["t"], dt=d["t"] - prev["t"] if prev else None, good=d["good"], paused=d["paused"],
                                 post_warm=d["post_warm"], legs=legs, R=any(l["qm"] == "R" for l in legs)))
                prev = d
            data[(cell, arm, seed, int(rep))] = dict(recs=recs, qd=qd, secs=secs)

# 0) reproduce attr48's priority-chain UNDERREAD share (checks this parse against attribution.txt)
P("\n== check: attr48 priority-chain UNDERREAD share per CAND row (compare attribution.txt)")
for cell in ("c2-100", "c8-100"):
    s = []
    for (c, a, sd, rp), v in sorted(data.items()):
        if c != cell or a != "CAND":
            continue
        cnt = {"STORE": 0, "QUINN": 0, "PROBERTT": 0, "UNDERREAD": 0, "OTHER": 0}; n = 0
        for r in v["recs"]:
            for i, l in enumerate(r["legs"]):
                if not l["act"]:
                    continue
                n += 1
                if r["paused"] is not None and r["paused"] >= 50 and l["B"] is not None and l["B"] < 98: cnt["STORE"] += 1
                elif l["B"] is not None and l["B"] >= 98 and l["qm"] in ("B", "S", "D"): cnt["QUINN"] += 1
                elif l["qm"] == "R": cnt["PROBERTT"] += 1
                elif l["qbw"] is not None and l["qbw"] < 0.8 * LINK[cell][i]: cnt["UNDERREAD"] += 1
                else: cnt["OTHER"] += 1
        s.append(f"s{sd}r{rp} n={n} U={cnt['UNDERREAD']/n:.3f}")
    P(f"  {cell}: " + "  ".join(s))

# 1) run-level delivered rate per leg
P("\n== per-leg delivered rate (netem Sent bytes / run seconds), median over rows")
for cell in ("c2-100", "c8-100"):
    for arm in ("BASE", "CAND"):
        rows = [v for (c, a, *_), v in data.items() if c == cell and a == arm]
        for i, link in enumerate(LINK[cell]):
            rate = st.median(v["qd"][i][0] / v["secs"] for v in rows)
            P(f"  {cell} {arm} leg{i}: {rate/1e6:.3f} MB/s = {rate/link:.3f} x link (0.8 x link = {0.8*link/1e6:.2f} MB/s)")

# 2) raw UNDER indicator on non-R active lines, per leg
P("\n== raw UNDER-READ (qbw < 0.8 x link) on CAND non-ProbeRtt lines with the leg active, per leg (pooled over 12 rows)")
q_ = lambda a, p: a[int(p * (len(a) - 1))] if a else float("nan")
for cell in ("c2-100", "c8-100"):
    for i, link in enumerate(LINK[cell]):
        n = u = 0; gw = gu = 0.0; dw = du = 0.0; ratios = []; rr = []
        for (c, a, *_), v in data.items():
            if c != cell or a != "CAND":
                continue
            bpp = v["qd"][i][0] / v["qd"][i][1]
            for r in v["recs"]:
                l = r["legs"][i]
                if r["R"] or not l["act"] or l["qbw"] is None or r["good"] is None:
                    continue
                U = l["qbw"] < 0.8 * link
                n += 1; u += U; gw += r["good"]; gu += r["good"] * U; dw += l["dtx"]; du += l["dtx"] * U
                ratios.append(l["qbw"] / link)
                if r["dt"] and l["dtx"]:
                    rr.append(l["qbw"] / (l["dtx"] * bpp / r["dt"]))
        ratios.sort(); rr.sort()
        P(f"  {cell} leg{i}: lines {n}  UNDER line share {u/n:.3f}  goodput-weighted {gu/gw:.3f}  leg-delivery-weighted {du/dw:.3f}"
          f"  | qbw/link p10 {q_(ratios,.1):.3f} p50 {q_(ratios,.5):.3f} p90 {q_(ratios,.9):.3f}  | qbw/(leg's own line rate) p10 {q_(rr,.1):.2f} p50 {q_(rr,.5):.2f} p90 {q_(rr,.9):.2f}")

# 3) deficit decomposition
P("\n== goodput deficit decomposition (line goodput 'good=', Mbit/s; contribution = share x (g_ref - class mean); classes sum to g_ref - CAND mean)")
for cell in ("c2-100", "c8-100"):
    gref = st.mean(r["good"] for (c, a, *_), v in data.items() if c == cell and a == "BASE" for r in v["recs"] if r["good"] is not None)
    cls = {}
    for (c, a, *_), v in data.items():
        if c != cell or a != "CAND":
            continue
        for r in v["recs"]:
            if r["good"] is None:
                continue
            U = [l["act"] and l["qbw"] is not None and l["qbw"] < 0.8 * LINK[cell][i] for i, l in enumerate(r["legs"])]
            k = "R" if r["R"] else (("UNDER(" + "+".join(f"L{i}" for i, x in enumerate(U) if x) + ")") if any(U) else "nonR-ok")
            cls.setdefault(k, []).append(r["good"])
    N = sum(len(x) for x in cls.values()); cmean = st.mean(g for x in cls.values() for g in x)
    P(f"  {cell}: g_ref(BASE) {gref:.2f}  CAND mean {cmean:.2f}  total deficit {gref-cmean:+.2f} ({100*(gref-cmean)/gref:+.2f}%)")
    tot = 0.0
    for k, x in sorted(cls.items()):
        sh = len(x) / N; con = sh * (gref - st.mean(x)); tot += con
        P(f"    {k:14s} share {sh:.3f} mean {st.mean(x):7.2f}  contribution {con:+.3f} Mbit/s ({100*con/gref:+.2f}% of g_ref)")
    P(f"    sum of contributions {tot:+.3f} (must equal the total deficit)")

# 3b) per row: UNDER share vs goodput
P("\n== c8 CAND per row: UNDER share per leg (non-R active lines) vs row goodput")
pts = []
for (c, a, sd, rp), v in sorted(data.items()):
    if c != "c8-100" or a != "CAND":
        continue
    sh = []
    for i in (0, 1):
        x = [r["legs"][i]["qbw"] < 0.8 * LINK[c][i] for r in v["recs"] if not r["R"] and r["legs"][i]["act"] and r["legs"][i]["qbw"] is not None]
        sh.append(sum(x) / len(x) if x else float("nan"))
    mb = 600e6 * 8 / v["secs"] / 1e6
    pts.append((sh[0], sh[1], mb)); P(f"  s{sd} r{rp}: U_L0 {sh[0]:.3f} U_L1 {sh[1]:.3f}  goodput {mb:.2f}")
A = np.array(pts)
for i in (0, 1):
    if A[:, i].std() > 0:
        P(f"  corr(U_L{i}, row goodput) = {np.corrcoef(A[:, i], A[:, 2])[0, 1]:+.2f} (n=12)")

# 4) R2a coincidence
P("\n== R2a (qcwnd < BDP) vs UNDER, per leg, CAND non-R active lines: 2x2 and the independence expectation")
for cell in ("c8-100", "c2-100"):
    for i in range(len(LINK[cell])):
        a = b = c_ = d = 0
        for (c, ar, *_), v in data.items():
            if c != cell or ar != "CAND":
                continue
            for r in v["recs"]:
                l = r["legs"][i]
                if r["R"] or not l["act"] or l["qbw"] is None or l["cw"] is None:
                    continue
                U = l["qbw"] < 0.8 * LINK[cell][i]; W = l["cw"] < BDP[cell][i]
                a += U and W; b += U and not W; c_ += (not U) and W; d += (not U) and (not W)
        n = a + b + c_ + d
        if n:
            exp = (a + b) * (a + c_) / n
            P(f"  {cell} leg{i}: n={n}  UNDER&R2a={a} (indep. exp {exp:.1f})  UNDER only={b}  R2a only={c_}  neither={d}"
              f"  | P(R2a)={(a+c_)/n:.3f} P(R2a|UNDER)={a/max(a+b,1):.3f} P(UNDER|R2a)={a/max(a+c_,1):.3f}")
for key in (("c8-100", "CAND", "42", 1),):
    v = data[key]; n = 0; bel = [0, 0]
    for r in v["recs"]:
        if not r["post_warm"] or any(l["cw"] is None for l in r["legs"]):
            continue
        n += 1
        for i in (0, 1):
            bel[i] += r["legs"][i]["cw"] < BDP["c8-100"][i]
    P(f"  check ledger r2a {key}: per leg {bel[0]/n:.4f} / {bel[1]/n:.4f} (ledger 0.0933 / 0.0466), n={n} (ledger 193)")

# 5) timing around UNDER onsets
P("\n== c8 CAND: mean line goodput around UNDER onsets (not-under -> under, non-R), per leg; offset in [DIAG] lines (~0.25 s)")
for i in (0, 1):
    acc = {k: [] for k in range(-4, 5)}
    for (c, a, *_), v in data.items():
        if c != "c8-100" or a != "CAND":
            continue
        rs = v["recs"]
        U = [(r["legs"][i]["qbw"] is not None and r["legs"][i]["qbw"] < 0.8 * LINK[c][i]) for r in rs]
        for j in range(1, len(rs)):
            if U[j] and not U[j - 1] and not rs[j]["R"]:
                for k in acc:
                    if 0 <= j + k < len(rs) and rs[j + k]["good"] is not None:
                        acc[k].append(rs[j + k]["good"])
    P(f"  leg{i}: onsets {len(acc[0])}  " + " ".join(f"{k:+d}:{st.mean(x):.1f}" for k, x in acc.items() if x))
open(os.path.join(D, "diag-c8/q2_out.txt"), "w").write("\n".join(out) + "\n")
