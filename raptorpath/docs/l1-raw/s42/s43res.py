"""Status §43 follow-up (offline, read-only): the residence-time store law on the p27/p34 static DEF rows.

    ask_g(n) = g · Σᵢ Lᵢ · ( R̄ − (Tminᵢ − RTpropᵢ) ),   R̄ = occupancy / D,   g ∈ {0.8, 0.9, 1.0, 1.1}

(g = 1.1 is the registered (1+q); the old law ask = 1.1·ΣLᵢ(RTpropᵢ + Sᵢ), S = Tp50 − Tmin, is printed as the gate.)
Usage: python -I s43res.py <worktree>/raptorpath/docs/l1-raw/loop-diag   (imports s42noise.py from this directory)

Definitions (stated in the output too):
- occupancy = `[DIAG] win=` numerator (store_len = sent_store retained − SACK-released, generation=false), a SNAPSHOT
  at the window close, not a window average.
- D = Σ legs `[LOOPDIAG] dlv_rate` = delivered symbols / window (the `record_delivery` count, one datagram each),
  symbols/s, same units as L (netem rate ÷ 1 279 B). Cross-check: the `[DIAG] cum=` ack delta ÷ `wait[us=]`.
- R̄ per row = mean(occ) / mean(D) over windows (ratio of means, Little); per window R̄_w = occ_w / D_w.
- Windows: t >= 1.0 s, every non-idle leg n > 0 (s42noise's rule). Map value per cap = mean over LIVE rows of row values;
  Tmin, Tp50, RTprop per leg = the row's ld_p<i> medians (as ask27).
- Anchors: (a) fresh RTprop = Copa rtprop_us (row median); (b) expired RTprop = the row's lowest post-warm-up
  rtt_min_us per leg (as s42rtprop.py).
- c8 (two legs, one store): ask = g·Σᵢ Lᵢ·(R̄ − brakeᵢ), R̄ cell-level. One LIVE row per cap at most caps.
"""
import math, os, re, statistics as st, sys
from collections import defaultdict
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import s42noise as s

GS = (0.8, 0.9, 1.0, 1.1)
KNEE = {"c2-100": 192, "c1s-400": 1024, "c8-100": 384}
DG = re.compile(r"\[DIAG\] t=([\d.]+)s win=(\d+)/(\d+) .*?cum=(\d+)/(\d+)/(\d+) .*? us=(\d+) busy")
LDR = re.compile(r"\[LOOPDIAG\] t=([\d.]+)s p(\d+) n=(\d+) ov=(\d+) rtt_min_us=(\S+) rtt_p10_us=(\S+) rtt_p50_us=(\S+) "
                 r"rtprop_us=(\S+) jit_us=(\S+) srtt_us=(\S+) dlv=(\d+) dlv_rate=(\S+)")
CASES = [("p27", "c2-100"), ("p34", "c1s-400"), ("p27", "c1s-400"), ("p27", "c8-100")]


def fnum(x):
    return None if x == "-" else float(x)


def parse_row(path, nleg, act):
    """-> list of windows: dict(t, occ, cap, D, Dack, legs={i: (n, ov, tmin, tp50, rtprop, srtt, dlv_rate)})."""
    lds, dgs = defaultdict(dict), {}
    for l in open(path, errors="replace"):
        m = LDR.search(l)
        if m:
            g = m.groups()
            lds[float(g[0])][int(g[1])] = (int(g[2]), int(g[3]), fnum(g[4]), fnum(g[6]), fnum(g[7]), fnum(g[9]),
                                           float(g[11]))
            continue
        m = DG.search(l)
        if m:
            dgs[float(m.group(1))] = (int(m.group(2)), int(m.group(3)), int(m.group(6)), int(m.group(7)))
    out, prev = [], None
    for t in sorted(dgs):
        occ, cap, ack, us = dgs[t]
        dack = (ack - prev) / us * 1e6 if prev is not None and us > 0 else None
        prev = ack
        if t < 1.0 or t not in lds:
            continue
        legs = lds[t]
        if any(i not in legs or legs[i][0] == 0 or None in (legs[i][2], legs[i][3], legs[i][4]) for i in act):
            continue
        D = sum(legs[i][6] for i in range(nleg) if i in legs)
        if D <= 0:
            continue
        out.append(dict(t=t, occ=occ, cap=cap, D=D, Dack=dack, legs=legs))
    # transfer end: the last tick can land after the store drained (occ 0-1 seen at c2 192, c8 384). Rule: drop the
    # row's LAST window when its occupancy is below 0.1 of the cap (a drained store). Listed under TRIMMED at the end.
    if out and out[-1]["occ"] < 0.1 * out[-1]["cap"]:
        TRIMMED.append((os.path.basename(path), out[-1]["t"], out[-1]["occ"]))
        out.pop()
    return out


TRIMMED = []


def mean(v):
    v = [x for x in v if x is not None]
    return st.mean(v) if v else None


def crossings(a):
    ns = sorted(a)
    xs = []
    for lo, hi in zip(ns, ns[1:]):
        flo, fhi = a[lo] - lo, a[hi] - hi
        if (flo >= 0) != (fhi >= 0):
            x = lo + flo / (flo - fhi) * (hi - lo)
            xs.append(("down" if flo >= 0 else "up", x, (a[hi] - a[lo]) / (hi - lo), lo, hi))
    return xs


def interp(curve, x):
    ns = sorted(curve)
    if x <= ns[0]:
        return curve[ns[0]]
    if x >= ns[-1]:
        return curve[ns[-1]]
    for lo, hi in zip(ns, ns[1:]):
        if lo <= x <= hi:
            return curve[lo] + (curve[hi] - curve[lo]) * (x - lo) / (hi - lo)


def cell(led, c, drop_outliers=False):
    allrows = [r for r in s.rows(led) if r["cell"] == c and r["arm"].startswith("DEF")]
    live = defaultdict(list)
    for r in allrows:
        if r["status"] == "LIVE":
            live[int(r["arm"][3:])].append(r)
    dropped = []
    if drop_outliers:   # a row under 0.6x the median goodput of its cap's other LIVE rows
        for n, rs in live.items():
            keep = [r for r in rs if not (len(rs) > 1 and
                    r["mbps"] < 0.6 * st.median(x["mbps"] for x in rs if x is not r))]
            dropped += [f"DEF{n} s{r['seed']}r{r['rep']} ({r['mbps']:.0f} Mbit/s, {r['seconds']:.0f} s)"
                        for r in rs if r not in keep]
            live[n] = keep
    links = s.PATH_LINK[c]
    L = [lb / s.W_WIRE for lb in links]
    nleg = len(links)
    grid = [n for n in s.GRID[led][c] if live[n]]
    knee = KNEE[c]
    tag = ("  [OUTLIER ROWS DROPPED: " + ("; ".join(dropped) or "none") + "]") if drop_outliers else "  [all LIVE rows, as s42]"
    print(f"\n{'=' * 100}\n== {led} {c}{tag}  L ={', '.join(f'{x:.0f}' for x in L)} dgram/s; knee n* = {knee}; "
          f"knee band [n*/1.5, 1.5n*] = [{knee / 1.5:.0f}, {knee * 1.5:.0f}]")
    if c == "c8-100":
        print("   THIN: one LIVE row at most caps (WITNESS-FAIL rows = idle slow leg are excluded); R̄ is cell-level.")
    if c == "c1s-400":
        print("   CAVEAT: every c1 window overflows LOOPDIAG's 8 192-sample cap, so Tmin/Tp50/brake come from the first "
              "~40 % of each window; occ, D, srtt are not truncated. R̄ − Tp50 at c1 is confounded by that.")
    cap = {}
    for n in grid:
        per = []
        for r in live[n]:
            f = os.path.join(s.BASE, s.LEDGER[led], "diag-ld", f"{c}-{r['arm']}-s{r['seed']}-r{r['rep']}-a1-c.log")
            idle = [i for i in range(nleg) if r.get("ld_pn", {}).get(str(i), [1])[0] == 0]
            act = [i for i in range(nleg) if i not in idle]
            ws = parse_row(f, nleg, act)
            if not ws:
                continue
            floor = []
            for i in range(nleg):
                v = [w["legs"][i][2] for w in ws if i in w["legs"] and w["legs"][i][0] > 0 and w["legs"][i][2]]
                floor.append(min(v) if v else None)
            med = lambda i, k: r.get(f"ld_p{i}", {}).get(k)
            rb = sum(w["occ"] for w in ws) / sum(w["D"] for w in ws) * 1e6       # µs
            dack = [w["Dack"] for w in ws if w["Dack"]]
            per.append(dict(
                r=r, ws=ws, act=act, Rbar=rb,
                RnD=n / mean([w["D"] for w in ws]) * 1e6,
                occn=st.median(w["occ"] / n for w in ws), full=sum(w["occ"] >= n for w in ws) / len(ws),
                D=mean([w["D"] for w in ws]), Dack=mean(dack),
                srtt=[mean([w["legs"][i][5] for w in ws if i in w["legs"]]) for i in range(nleg)],
                Dl=[mean([w["legs"][i][6] for w in ws if i in w["legs"]]) or 0.0 for i in range(nleg)],
                tmin=[med(i, "rtt_min_us") for i in range(nleg)], tp50=[med(i, "rtt_p50_us") for i in range(nleg)],
                rtp=[med(i, "rtprop_us") for i in range(nleg)], floor=floor, mbps=r["mbps"]))
        if not per:
            continue
        Dl = [mean([p["Dl"][i] for p in per]) for i in range(nleg)]
        legs = []
        for i in range(nleg):
            g_ = lambda k: mean([p[k][i] for p in per if i in p["act"]])
            legs.append(dict(tmin=g_("tmin"), tp50=g_("tp50"), rtp=g_("rtp"), floor=g_("floor"), srtt=g_("srtt")))
        cap[n] = dict(per=per, rows=len(per), Rbar=mean([p["Rbar"] for p in per]), RnD=mean([p["RnD"] for p in per]),
                      occn=mean([p["occn"] for p in per]), full=mean([p["full"] for p in per]),
                      D=mean([p["D"] for p in per]), Dack=mean([p["Dack"] for p in per]),
                      mbps=mean([p["mbps"] for p in per]), legs=legs,
                      win=sum(len(p["ws"]) for p in per),
                      srtt_w=sum(Dl[i] * legs[i]["srtt"] for i in range(nleg) if legs[i]["srtt"]) / sum(
                          Dl[i] for i in range(nleg) if legs[i]["srtt"]))
    grid = sorted(cap)

    # ---- 1. Little / premise table
    print("\n-- 1. Residence time vs loop time (ms; per cap = mean over rows). Leg p0 unless stated.")
    print("   R̄ = mean(occ)/mean(D); n/D = cap/D; srtt = mean of srtt_us SNAPSHOTS (1/8 EWMA, ~8 samples): the only "
          "mean-loop-time proxy in the ledger (no per-window mean is logged).")
    print(f"   {'n':>5} {'rows':>4} {'win':>4} {'occ/n':>5} {'full':>4} {'D/L':>5} {'Dack/D':>6} {'R̄':>7} {'n/D':>7}"
          f" {'srtt*':>7} {'Tp50':>7} {'Tmin':>7} {'RTprop':>7} {'floor':>7} {'τ=Tp50-brk':>10} {'R̄-Tp50':>7}"
          f" {'R̄/Tp50':>6} {'R̄/τ':>5} {'R̄/srtt':>6} {'R̄/srtt_w':>8} {'R̄-brk_f':>8} {'R̄-brk_x':>8} {'brk_f/R̄':>8} {'brk_x/R̄':>8}")
    for n in grid:
        d = cap[n]
        for i, lg in enumerate(d["legs"]):
            if lg["tp50"] is None:
                continue
            tau = lg["tp50"] - (lg["tmin"] - lg["rtp"])
            Lc = sum(L)
            print(f"   {n:5d} {d['rows']:4d} {d['win']:4d} {d['occn']:5.2f} {d['full']:4.2f} {d['D'] / Lc:5.3f}"
                  f" {d['Dack'] / d['D']:6.3f} {d['Rbar'] / 1e3:7.2f} {d['RnD'] / 1e3:7.2f} {lg['srtt'] / 1e3:7.2f}"
                  f" {lg['tp50'] / 1e3:7.2f} {lg['tmin'] / 1e3:7.2f} {lg['rtp'] / 1e3:7.2f} {lg['floor'] / 1e3:7.2f}"
                  f" {tau / 1e3:10.2f} {(d['Rbar'] - lg['tp50']) / 1e3:+7.2f} {d['Rbar'] / lg['tp50']:6.3f}"
                  f" {d['Rbar'] / tau:5.2f} {d['Rbar'] / lg['srtt']:6.3f} {d['Rbar'] / d['srtt_w']:6.3f}"
                  f" {(d['Rbar'] - (lg['tmin'] - lg['rtp'])) / 1e3:8.2f} {(d['Rbar'] - (lg['tmin'] - lg['floor'])) / 1e3:8.2f}"
                  f" {(lg['tmin'] - lg['rtp']) / d['Rbar']:8.3f} {(lg['tmin'] - lg['floor']) / d['Rbar']:8.3f}"
                  + (f"  [leg p{i}]" if nleg > 1 else ""))
    print("   (srtt_w = D-weighted mean of the legs' srtt*, the cell-level loop time Little compares R̄ with)")
    print("   (D/L uses ΣL; brk_f = Tmin − RTprop fresh, brk_x = Tmin − floor expired; R̄-brk = the new law's time term)")

    # ---- maps
    def tau_new(d, i, anchor):
        lg = d["legs"][i]
        if lg["tp50"] is None:
            return 0.0
        a = lg["rtp"] if anchor == "fresh" else lg["floor"]
        return d["Rbar"] - (lg["tmin"] - a)

    def ask(n, g, anchor):
        d = cap[n]
        return g * sum(L[i] * tau_new(d, i, anchor) / 1e6 for i in range(nleg) if d["legs"][i]["tp50"] is not None)

    old = {n: s.map_ask([p["r"] for p in cap[n]["per"]], c) for n in grid}
    old = {n: a for n, a in old.items() if a}
    gp = {n: cap[n]["mbps"] for n in grid}
    tp = {n: cap[n]["legs"][0]["tp50"] / 1e3 for n in grid}
    gmax = max(gp.values())
    print(f"\n-- gate: old law 1.1·ΣL(RTprop+S): " + ", ".join(f"{n}:{a:.0f}" for n, a in sorted(old.items()))
          + " | crossings " + "; ".join(f"{k} {x:.1f} slope {sl:.2f}" for k, x, sl, *_ in crossings(old)))
    print(f"   static goodput (Mbit/s, mean of LIVE rows): " + ", ".join(f"{n}:{gp[n]:.1f}" for n in grid)
          + f"; Gmax {gmax:.1f}")
    print(f"   static Tp50 p0 (ms): " + ", ".join(f"{n}:{tp[n]:.1f}" for n in grid))

    print("\n-- 2. Map ask_g(n), crossings, goodput fraction and Tp50 at the first stable crossing, ask/n above the knee")
    fps = {}
    for anchor in ("fresh", "expired"):
        print(f"\n   anchor {anchor} ({'Copa rtprop_us row median' if anchor == 'fresh' else 'row lowest post-warm-up rtt_min_us'})")
        for g in GS:
            a = {n: ask(n, g, anchor) for n in grid}
            xs = crossings(a)
            if a[grid[0]] < grid[0]:
                cls = "FLOOR (ask < n at the lowest cap)"
            elif not xs:
                cls = "CEILING (ask >= n at every cap)"
            else:
                cls = None
            down = [x for x in xs if x[0] == "down"]
            line = f"   g={g:.1f}  ask: " + ", ".join(f"{n}:{a[n]:.0f}" for n in grid)
            print(line)
            if cls:
                print(f"          {cls}")
            print(f"          crossings: " + ("; ".join(f"{k}({'stable' if k == 'down' else 'unstable'}) {x:.0f} slope {sl:.2f}"
                                                    for k, x, sl, *_ in xs) or "none"))
            if down:
                k, x, sl, lo, hi = down[0]
                G = interp(gp, x)
                inband = knee / 1.5 <= x <= knee * 1.5
                print(f"          first stable point {x:.0f} (slope {sl:.2f}): goodput {G:.1f} = {G / gmax:.3f} Gmax, "
                      f"Tp50 {interp(tp, x):.1f} ms; lands in the knee band: {'yes' if inband else 'no'}")
                fps[(anchor, g)] = (x, sl, lo, hi)
                DL = interp({n: cap[n]["D"] / sum(L) for n in grid}, x)
                bR = interp({n: sum(L[i] * (cap[n]["legs"][i]["tmin"] - (cap[n]["legs"][i]["rtp"] if anchor == "fresh"
                             else cap[n]["legs"][i]["floor"])) for i in range(nleg) if cap[n]["legs"][i]["tp50"] is not None)
                             / sum(L) / cap[n]["Rbar"] for n in grid}, x)
                print(f"          there (interpolated): D/L {DL:.3f}, brake/R̄ {bR:.3f} (L-weighted over legs); "
                      f"the brief's fixed point D = L, brake/R̄ = (g-1)/g = {(g - 1) / g:+.3f}")
            for j, x in enumerate(xs):
                if x[0] == "up":
                    nxt = xs[j + 1][1] if j + 1 < len(xs) else None
                    print(f"          above the unstable {x[1]:.0f}: ask >= n, n rises "
                          + (f"to the next stable point {nxt:.0f}" if nxt else "to the clamp (runaway)"))
            print(f"          ask/n at caps >= knee: " + ", ".join(f"{n}:{a[n] / n:.3f}" for n in grid if n >= knee)
                  + "   | g·ΣL/D there: " + ", ".join(f"{n}:{g * sum(L) / cap[n]['D']:.3f}" for n in grid if n >= knee))

    # ---- floor
    print("\n   p90 L̂ (status §36 addendum / §40, capture ledgers, not joined here): from its crossover up (c2 >= 64,"
          " c1s >= 128, c8 fast >= 64, c8 slow >= 256) it reads 1.000-1.038 L, so on every cap of these grids except"
          " c8's slow leg below 256, ask_p90 = (1.00-1.04)·ask_oracle and g_min / g_knee divide by that factor."
          " Below the crossover p90 L̂/D = 1.6-3.9. The 'L̂ = D' lines below are the circular stand-in, not the p90.")
    print("\n-- Floor: ask_1/n per cap (g = 1, so ask_g/n = g × this); the lowest g with ask/n > 1 at a cap is n/ask_1; g_min = max of that over caps below the knee (the fixed point then sits at or above the highest cap below the knee)")
    for anchor in ("fresh", "expired"):
        a1 = {n: ask(n, 1.0, anchor) for n in grid}
        # L̂ = D (the circular read; what any delivered-rate L̂ gives below the dispersion crossover)
        aD = {n: sum(cap[n]["D"] * L[i] / sum(L) * tau_new(cap[n], i, anchor) / 1e6 for i in range(nleg)
                     if cap[n]["legs"][i]["tp50"] is not None) for n in grid}
        below = [n for n in grid if n < knee]
        print(f"   {anchor}: ask_1/n  " + ", ".join(f"{n}:{a1[n] / n:.3f}" for n in grid)
              + f"\n          g_min (oracle L) over caps < knee = {max(n / a1[n] for n in below):.3f}"
              + f" (per cap " + ", ".join(f"{n}:{n / a1[n]:.3f}" for n in below) + ")"
              + f"\n          L̂ = D: ask_1/n " + ", ".join(f"{n}:{aD[n] / n:.3f}" for n in grid)
              + f"; g_min over caps < knee = {max(n / aD[n] for n in below):.3f}"
              + f"\n          g that puts ask = n at the knee cap {knee}: oracle L {knee / a1[knee]:.3f}, L̂ = D {knee / aD[knee]:.3f}")

    # ---- 3. noise
    print("\n-- 3. Per-window noise of ask_w = g·ΣLᵢ(occ_w/D_w − (Tmin_w − RTprop_w)) (fresh: window rtprop_us;"
          " expired: row floor), at the caps bracketing each first stable point")
    for anchor in ("fresh", "expired"):
        for g in GS:
            fp = fps.get((anchor, g))
            if not fp:
                print(f"   {anchor} g={g}: no stable crossing")
                continue
            x, sl, lo, hi = fp
            sds, parts = [], []
            for n in (lo, hi):
                d = cap[n]
                series = []
                for p in d["per"]:
                    srs = []
                    for w in p["ws"]:
                        tot = 0.0
                        for i in p["act"]:
                            lgw = w["legs"][i]
                            rp = lgw[4] if anchor == "fresh" else p["floor"][i]
                            tot += L[i] * (w["occ"] / w["D"] * 1e6 - (lgw[2] - rp)) / 1e6
                        srs.append(g * tot)
                    series.append(srs)
                mapsgn = 1 if ask(n, g, anchor) > n else -1
                k = m = 0
                cvs, es, rm = [], [], []
                for srs in series:
                    k += sum(1 for v in srs if (1 if v > n else -1) == mapsgn)
                    m += len(srs)
                    pos = [v for v in srs if v > 0]
                    if len(srs) > 1:
                        cvs.append(st.pstdev(srs) / st.mean(srs))
                    e = [math.log(v / n) for v in pos]
                    es.append(e)
                    if e:
                        rm.append(st.mean(e))
                within = [((v - st.mean(e)) ** 2) for e in es if len(e) > 1 for v in e]
                v1 = st.mean(within) if within else 0.0
                sb2 = max(0.0, st.variance(rm) - v1 / st.mean(len(e) for e in es)) if len(rm) >= 2 else 0.0
                sds.append(math.sqrt(v1 + sb2))
                nonpos = sum(1 for srs in series for v in srs if v <= 0)
                parts.append(f"n={n} rows={len(series)} win={m} cv/row {mean(cvs):.3f} sign agree {k}/{m}={k / m:.2f}"
                             f" sd_within(e) {math.sqrt(v1):.3f} sd_between {math.sqrt(sb2):.3f}"
                             + (f" ask_w<=0 in {nonpos} windows" if nonpos else ""))
            sd = st.mean(sds)
            pred = f"{sd / math.sqrt(1 - sl ** 2):.3f}" if abs(sl) < 1 else "undefined (|slope| >= 1)"
            print(f"   {anchor} g={g}: fp {x:.0f} slope {sl:.2f} | " + " | ".join(parts)
                  + f" | bracket sd(e) {sd:.3f} -> predicted closed-loop sd(ln n) {pred}")

    # Dack lumpiness: cv of per-window occ/Dack vs occ/D at the knee cap
    n = min(grid, key=lambda z: abs(z - knee))
    a = [w["occ"] / w["D"] for p in cap[n]["per"] for w in p["ws"]]
    b = [w["occ"] / w["Dack"] for p in cap[n]["per"] for w in p["ws"] if w["Dack"]]
    print(f"\n   D source at n={n}: cv(occ/D_dlv) {st.pstdev(a) / st.mean(a):.3f} vs cv(occ/D_ack) "
          f"{st.pstdev(b) / st.mean(b):.3f}; mean ratio D_ack/D_dlv {cap[n]['Dack'] / cap[n]['D']:.3f}")


if __name__ == "__main__":
    sys.stdout.reconfigure(encoding="utf-8")
    s.BASE = sys.argv[1]
    print(__doc__)
    for led, c in CASES:
        cell(led, c)
    print("\n\n######## c1s again with outlier rows dropped (goodput < 0.6x the median of the cap's other LIVE rows)")
    for led, c in (("p34", "c1s-400"), ("p27", "c1s-400")):
        cell(led, c, drop_outliers=True)
    print("\nTRIMMED end-of-transfer windows (file, t, occ): "
          + "; ".join(f"{a} t={b} occ={c}" for a, b, c in sorted(set(TRIMMED))))
