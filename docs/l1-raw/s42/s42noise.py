"""Status §42 Next-1: per-window noise of the store law ask = (1+q) sum_i L_i (RTprop_i + S_i).

Read-only on committed ledgers. Usage: python -I s42noise.py <repo>/raptorpath/docs/l1-raw/loop-diag
Rows: LIVE DEF<n> rows of p27/ld.log and p34/run/ld.log (LDROW json). Client log
diag-ld/{cell}-{arm}-s{seed}-r{rep}-a1-c.log. Windows: [LOOPDIAG] t >= 1.0 s (parser WARM_S)
and n > 0 (parser loopdiag_summary). Fields: rtprop_us (Copa min_rtt, a 10 s windowed min),
rtt_p50_us (Tp50), rtt_min_us (Tmin); S_w = Tp50_w - Tmin_w.
Two-leg cells: a window counts only if every leg has n > 0 at that t (dropped share reported).
"""
import glob, json, math, os, re, sys, statistics as st
from collections import defaultdict

BASE = sys.argv[1]
W_WIRE, QG = 1279.0, 1.1
PATH_LINK = {"c1s-400": [125e6], "c2-100": [12.5e6], "c1d-400": [125e6, 125e6], "c8-100": [12.5e6, 2.5e6]}
GRID = {
    "p27": {"c2-100": [64, 96, 128, 192, 256, 384, 512, 1024],
            "c8-100": [128, 192, 256, 384, 512, 768, 1024, 2048],
            "c1s-400": [192, 256, 384, 512, 768, 1024, 1536, 2048],
            "c1d-400": [384, 512, 768, 1024, 1536, 2048, 3072, 4096]},
    "p34": {"c1s-400": [256, 384, 512, 768, 1024, 1536, 2048],
            "c1d-400": [384, 512, 768, 1024, 1536, 2048]},
}
LEDGER = {"p27": "p27", "p34": "p34/run"}
LD = re.compile(r"\[LOOPDIAG\] t=([\d.]+)s p(\d+) n=(\d+) ov=(\d+) rtt_min_us=(\S+) rtt_p10_us=(\S+) "
                r"rtt_p50_us=(\S+) rtprop_us=(\S+)")
TRANSFER_S = {"c2-100": 8.9, "c8-100": 8.3, "c1s-400": 4.3, "c1d-400": 5.3}   # §30 MAIN transfers
TRANSFER_S34 = {"c1s-400": 4.7, "c1d-400": 5.3}                                # §34 R2 (c1s); c1d as §30
Z90 = 1.2816


def fnum(s):
    return None if s == "-" else float(s)


def rows(led):
    out = []
    for l in open(os.path.join(BASE, LEDGER[led], "ld.log")):
        if l.startswith("LDROW "):
            out.append(json.loads(l[6:]))
    return out


def windows(path, nleg):
    """-> (list of (t, [(rtprop, S) per leg]) with every leg n>0, total post-warm-up t count,
    {leg: windows with n==0}, ov windows)."""
    by_t = defaultdict(dict)
    for l in open(path, errors="replace"):
        m = LD.search(l)
        if not m:
            continue
        t = float(m.group(1))
        if t < 1.0:
            continue
        by_t[t][int(m.group(2))] = (int(m.group(3)), int(m.group(4)), fnum(m.group(5)), fnum(m.group(7)),
                                    fnum(m.group(8)))
    out, miss, ov = [], defaultdict(int), 0
    for t in sorted(by_t):
        legs, ok = [], True
        for i in range(nleg):
            d = by_t[t].get(i)
            if not d or d[0] == 0 or d[2] is None or d[3] is None or d[4] is None:
                miss[i] += 1
                ok = False
                continue
            if d[1] > 0:
                ov += 1
            legs.append((d[4], d[3] - d[2], d[2], d[3]))
        if ok:
            out.append((t, legs))
    return out, len(by_t), dict(miss), ov


def ask_of(legs, cell):
    return QG * sum(lb / W_WIRE * (rp + s) / 1e6 for lb, (rp, s, *_) in zip(PATH_LINK[cell], legs))


def map_ask(rs, cell):
    """§27 per_n27/ask27: mean over rows of the per-row per-leg medians (ld_p<i>), S = Tp50 - Tmin."""
    tot = 0.0
    for p, lb in enumerate(PATH_LINK[cell]):
        def m(k):
            v = [r.get(f"ld_p{p}", {}).get(k) for r in rs]
            v = [x for x in v if x is not None]
            return st.mean(v) if v else None
        rp, tp, tm = m("rtprop_us"), m("rtt_p50_us"), m("rtt_min_us")
        if None in (rp, tp, tm):
            return None
        tot += lb / W_WIRE * (rp + tp - tm) / 1e6
    return QG * tot


def fixed_point(ask):
    ns = sorted(n for n, a in ask.items() if a is not None)
    if ask[ns[0]] < ns[0]:
        return "FLOOR", None, None
    if ask[ns[-1]] >= ns[-1]:
        return "CEILING", None, None
    for lo, hi in zip(ns, ns[1:]):
        if ask[lo] >= lo and ask[hi] < hi:
            flo, fhi = ask[lo] - lo, ask[hi] - hi
            return "STABLE", lo + flo / (flo - fhi) * (hi - lo), (ask[hi] - ask[lo]) / (hi - lo)
    return "-", None, None


def wilson(k, n, z=1.96):
    if n == 0:
        return (float("nan"), float("nan"))
    p = k / n
    d = 1 + z * z / n
    c = (p + z * z / (2 * n)) / d
    h = z * math.sqrt(p * (1 - p) / n + z * z / (4 * n * n)) / d
    return (c - h, c + h)


def mov(xs, k):
    return [st.mean(xs[i:i + k]) for i in range(len(xs) - k + 1)]


def blocks(xs, k):
    return [st.mean(xs[i:i + k]) for i in range(0, len(xs) - k + 1, k)]


def agree(vals, n, sgn):
    k = sum(1 for a in vals if (1 if a > n else -1) == sgn)
    return k, len(vals)


def fmt_ag(k, n):
    if n == 0:
        return "   -          "
    lo, hi = wilson(k, n)
    return f"{k / n:4.2f} [{lo:.2f},{hi:.2f}] n={n}"


def analyse(led, cell):
    allrows = [r for r in rows(led) if r["cell"] == cell and r["arm"].startswith("DEF")]
    nleg = len(PATH_LINK[cell])
    grid = GRID[led][cell]
    print(f"\n==== {led} {cell} ====")
    # void / note inventory
    for r in allrows:
        if r["status"] != "LIVE" or r.get("notes"):
            print(f"  row {r['arm']} s{r['seed']} r{r['rep']}: {r['status']} {r.get('problems')} {r.get('notes')}")
    live = defaultdict(list)
    for r in allrows:
        if r["status"] == "LIVE":
            live[int(r["arm"][3:])].append(r)
    amap = {n: map_ask(live[n], cell) if live[n] else None for n in grid}
    kind, nfp, slope = fixed_point({n: a for n, a in amap.items() if a is not None})
    print(f"  map ask: " + ", ".join(f"{n}:{amap[n]:.0f}" if amap[n] else f"{n}:-" for n in grid))
    print(f"  fixed point: {kind} {nfp and round(nfp, 1)} slope {slope and round(slope, 3)}")
    res = {}
    for n in grid:
        rs = live[n]
        if not rs:
            continue
        sgn = 1 if amap[n] > n else -1
        per_row, all_a, all_e, legmiss, tot_t, ovw, durs = [], [], [], defaultdict(int), 0, 0, []
        rowseries = []
        for r in rs:
            # p34 idle-leg rows: the parser excludes the idle leg; keep only active legs for this row
            f = os.path.join(BASE, LEDGER[led], "diag-ld", f"{cell}-{r['arm']}-s{r['seed']}-r{r['rep']}-a1-c.log")
            ws, nt, miss, ov = windows(f, nleg)
            idle = [i for i in range(nleg) if r.get("ld_pn", {}).get(str(i), [1])[0] == 0]
            if idle:   # recompute with only active legs; links of idle legs contribute 0 (no S)
                act = [i for i in range(nleg) if i not in idle]
                ws2 = []
                wsall, _, _, _ = windows_any(f, nleg)
                for t, legs in wsall:
                    if all(legs[i] is not None for i in act):
                        ws2.append((t, legs))
                asks = [QG * sum(PATH_LINK[cell][i] / W_WIRE * (legs[i][0] + legs[i][1]) / 1e6 for i in act)
                        for _, legs in ws2]
            else:
                asks = [ask_of(legs, cell) for _, legs in ws]
            for i, c in miss.items():
                legmiss[i] += c
            tot_t += nt
            ovw += ov
            durs.append(r.get("seconds"))
            if not asks:
                continue
            rowseries.append(asks)
            all_a += asks
            all_e += [math.log(a / n) for a in asks]
            mean_a = st.mean(asks)
            per_row.append((r["seed"], r["rep"], len(asks), mean_a, st.pstdev(asks) / mean_a if len(asks) > 1 else 0,
                            st.median(asks), idle))
        if not all_a:
            continue
        e_med = st.median(all_e)
        sd_e = st.pstdev(all_e)
        # within-row level sd of e (pooled), and block-mean variances for gamma
        wsd = [st.pstdev([math.log(a / n) for a in s]) for s in rowseries if len(s) > 1]
        vW = {}
        for W in (1, 2, 4, 8):
            v = []
            for s in rowseries:
                b = [math.log(x / n) for x in blocks(s, W)]
                if len(b) >= 2:
                    mu = st.mean(b)
                    v += [(x - mu) ** 2 for x in b]
            vW[W] = st.mean(v) if v else float("nan")
        res[n] = dict(sgn=sgn, map=amap[n], per_row=per_row, all_a=all_a, all_e=all_e, e_med=e_med, sd_e=sd_e,
                      wsd=wsd, vW=vW, legmiss=dict(legmiss), tot_t=tot_t, ov=ovw, rowseries=rowseries, durs=durs)
    return grid, amap, (kind, nfp, slope), res, live


def windows_any(path, nleg):
    by_t = defaultdict(dict)
    for l in open(path, errors="replace"):
        m = LD.search(l)
        if not m or float(m.group(1)) < 1.0:
            continue
        by_t[float(m.group(1))][int(m.group(2))] = m
    out = []
    for t in sorted(by_t):
        legs = []
        for i in range(nleg):
            m = by_t[t].get(i)
            if not m or int(m.group(3)) == 0 or "-" in (m.group(5), m.group(7), m.group(8)):
                legs.append(None)
            else:
                legs.append((float(m.group(8)), float(m.group(7)) - float(m.group(5))))
        out.append((t, legs))
    return out, None, None, None


def report(led, cell):
    grid, amap, (kind, nfp, slope), res, live = analyse(led, cell)
    tr = (TRANSFER_S34 if led == "p34" else TRANSFER_S)[cell]
    print(f"  {'n':>5} {'rows':>4} {'win':>4} {'map':>6} {'med ask_w':>9} {'sgn':>3} {'|e_med|':>7} {'sd e':>5}"
          f" {'cv/row mean [range]':>22}  agree W=1 / mov2 / mov4 / mov8 / blk2 / blk4 / blk8 / row-mean")
    for n in grid:
        if n not in res:
            print(f"  {n:5d}  no live rows / windows")
            continue
        d = res[n]
        cvs = [x[4] for x in d["per_row"]]
        line = (f"  {n:5d} {len(d['per_row']):4d} {len(d['all_a']):4d} {d['map']:6.0f} {st.median(d['all_a']):9.0f}"
                f" {'+' if d['sgn'] > 0 else '-':>3} {abs(d['e_med']):7.3f} {d['sd_e']:5.3f}"
                f" {st.mean(cvs):6.3f} [{min(cvs):.3f}-{max(cvs):.3f}]")
        print(line)
        ags = []
        for lab, fn in [("W1", lambda s: s), ("mov2", lambda s: mov(s, 2)), ("mov4", lambda s: mov(s, 4)),
                        ("mov8", lambda s: mov(s, 8)), ("blk2", lambda s: blocks(s, 2)),
                        ("blk4", lambda s: blocks(s, 4)), ("blk8", lambda s: blocks(s, 8))]:
            k = m = 0
            for s in d["rowseries"]:
                kk, mm = agree(fn(s), n, d["sgn"])
                k += kk
                m += mm
            ags.append(f"{lab} {fmt_ag(k, m)}")
        rk = sum(1 for x in d["per_row"] if (1 if x[3] > n else -1) == d["sgn"])
        ags.append(f"row-mean {rk}/{len(d['per_row'])}")
        print("        " + " | ".join(ags))
        print("        rows: " + "; ".join(f"s{x[0]}r{x[1]} w={x[2]} mean={x[3]:.0f} cv={x[4]:.3f}"
                                        + (f" idle={x[6]}" if x[6] else "") for x in d["per_row"]))
        vW = d["vW"]
        gam = None
        pts = [(math.log(W), math.log(v)) for W, v in vW.items() if v > 0 and not math.isnan(v)]
        if len(pts) >= 3:
            mx = st.mean(p[0] for p in pts)
            my = st.mean(p[1] for p in pts)
            gam = -sum((x - mx) * (y - my) for x, y in pts) / sum((x - mx) ** 2 for x, _ in pts)
        s1 = math.sqrt(vW[1]) if vW[1] > 0 else float("nan")
        marg = abs(math.log(d["map"] / n))
        wstar = None
        if gam and gam > 0 and marg > 0:
            wstar = max(1.0, (Z90 * s1 / marg) ** (2.0 / gam))
        wsd1 = (Z90 * s1 / marg) ** 2 if marg > 0 else float("inf")
        print(f"        within-row sd(e) W=1 {s1:.3f}  var(blockmean) W=1,2,4,8: "
              + ", ".join(f"{vW[W]:.5f}" for W in (1, 2, 4, 8))
              + f"  gamma {gam and round(gam, 2)}  map margin |ln(map/n)| {marg:.3f}"
              + f"  W*_gamma {wstar and round(wstar, 1)}  W*_white {wsd1:.1f}")
        if wstar:
            sec = wstar * 0.25
            print(f"        W* = {sec:.2f} s; decisions per transfer third ({tr} s/3) = {tr / 3 / sec:.2f}")
        if d["legmiss"] or d["ov"]:
            print(f"        post-warm-up t's {d['tot_t']}, windows with leg missing {d['legmiss']}, ov>0 leg-windows {d['ov']}")
        print(f"        row seconds {d['durs']}")
    return res, (kind, nfp, slope)


if __name__ == "__main__":
    for led, cell in [("p27", "c2-100"), ("p27", "c8-100"), ("p27", "c1s-400"), ("p34", "c1s-400"),
                      ("p27", "c1d-400"), ("p34", "c1d-400")]:
        report(led, cell)
