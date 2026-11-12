"""s43win follow-ups (offline, read-only). Usage: python -I s43win2.py <loop-diag dir> <worktree root>
1. Closed-loop correction model at c2: T(n) = a + b·(n − 163), T = the law's time term RTprop + S (OCAP rtprop_us + s_us,
   without τ/b0), fitted on oracle43 c2 SL / SL-G12 / SL-B0 rows (last third: windows with t >= 2/3 of the row's last
   OCAP t). Closed-loop point n* solves n = G·L·(T(n) + τ):  n* = G·L·(a − 163·b + τ) / (1 − G·L·b).
2. τ = b0 + c: c window per cell = s43win's window − b0, intersected over c2, c8 (thin), c1s (p34 lower / oracle43 upper).
3. c1s at G = 0.9, τ ∈ {3.5, 5}: crossings of θ(n) = τ on oracle43's uncapped bins (fresh = live rtprop_us, expired =
   row floor) and on p34's static map.
4. qsp_us (P90 − P50 of the last 256 RTT samples) small-cap medians against b0 (Tmin_250ms − RTprop_10s)."""
import math, os, re, statistics as st, subprocess, sys
from collections import defaultdict
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import s42noise as s
import s43win as w
from s43res import interp

L2 = 12.5e6 / s.W_WIRE           # c2, datagrams/s
OC = re.compile(r"\[OCAP\] t=([\d.]+)s p0 law=\S+ n=(\d+) ask=([\d.]+) .*?rtprop_us=(\d+) tmin_us=(\d+) tp50_us=(\d+) "
                r"s_us=(\d+)")
QSP = re.compile(r" p(\d+):infl=.*?qsp_us=(\d+)/n(\d+)")
DGT = re.compile(r"\[DIAG\] t=([\d.]+)s")


def gshow(root, path):
    return subprocess.run(["git", "-C", root, "show", f"github/harness43:{path}"], capture_output=True, text=True,
                          errors="replace").stdout


def ols(xs, ys):
    mx, my = st.mean(xs), st.mean(ys)
    b = sum((x - mx) * (y - my) for x, y in zip(xs, ys)) / sum((x - mx) ** 2 for x in xs)
    return my - b * mx, b


def part1(root):
    print("\n#### 1. Closed-loop correction model (c2, oracle43)")
    arms = {"SL": (1.1, 0.0), "SL-G12": (1.2, 0.0), "SL-B0": (1.1, 2.47)}
    pts, wpts, per_arm = [], [], defaultdict(list)
    base = "raptorpath/docs/l1-raw/oracle43/diag-ld/"
    for arm, (G, tau) in arms.items():
        for sd, rp in (("42", 1), ("7", 1), ("42", 2)):
            txt = gshow(root, f"{base}c2-100-{arm}-s{sd}-r{rp}-a1-c.log")
            ws = [(float(g[0]), int(g[1]), int(g[3]) + int(g[6])) for g in OC.findall(txt) if float(g[0]) >= 1.0]
            tend = ws[-1][0]
            last = [x for x in ws if x[0] >= 2 / 3 * tend]
            n_end = st.median(x[1] for x in last)
            T_end = st.median(x[2] for x in last) / 1e3
            pts.append((arm, sd, rp, G, tau, n_end, T_end))
            wpts += [(x[1], x[2] / 1e3) for x in last]
            per_arm[arm].append(n_end)
    a0, b = ols([p[5] - 163 for p in pts], [p[6] for p in pts])
    aw, bw = ols([x[0] - 163 for x in wpts], [x[1] for x in wpts])
    print(f"  fit on row medians (9 rows): T(n) = {a0:.2f} ms + {b:.4f} ms/sym·(n − 163)")
    print(f"  fit on all last-third windows ({len(wpts)}): T(n) = {aw:.2f} + {bw:.4f}·(n − 163)   (advisor: 15.2 + 0.058)")
    print(f"  {'row':22s} {'G':>4} {'τ':>5} {'n_end':>6} {'T_end':>6} {'T_fit':>6} {'resid':>6}")
    for arm, sd, rp, G, tau, n, T in pts:
        print(f"  {arm + ' s' + sd + ' r' + str(rp):22s} {G:4.2f} {tau:5.2f} {n:6.0f} {T:6.2f} {a0 + b * (n - 163):6.2f} "
              f"{T - (a0 + b * (n - 163)):+6.2f}")
    res = [T - (a0 + b * (n - 163)) for *_, n, T in pts]
    print(f"  residual sd {st.pstdev(res):.2f} ms, max |resid| {max(abs(x) for x in res):.2f} ms")

    def npred(G, tau, a, bb):
        k = G * L2 / 1e3
        return k * (a - 163 * bb + tau) / (1 - k * bb) if k * bb < 1 else math.inf

    print("  predicted closed-loop point n* = G·L·(a − 163b + τ)/(1 − G·L·b) vs measured (median of the 3 rows' n_end):")
    for arm, (G, tau) in arms.items():
        print(f"    {arm:7s} G={G} τ={tau}: row-median fit {npred(G, tau, a0, b):6.1f} | window fit {npred(G, tau, aw, bw):6.1f}"
              f" | advisor line {npred(G, tau, 15.2, 0.058):6.1f} | measured {st.median(per_arm[arm]):.1f} "
              f"(rows {', '.join(f'{x:.1f}' for x in per_arm[arm])})")
    for G, tau in ((0.9, 5.0), (0.85, 5.0), (0.9, 3.5)):
        print(f"    PREDICTION G={G} τ={tau}: start-low n* = {npred(G, tau, a0, b):.0f} (window fit {npred(G, tau, aw, bw):.0f};"
              f" advisor line {npred(G, tau, 15.2, 0.058):.0f}); loop slope G·L·b = {G * L2 / 1e3 * b:.2f}")
    # start-high: the SH rows' own windows (expired anchor, n 1 500-2 048), ask/n under (0.9, 5)
    print("  start-high at (0.9, 5): ask/n = 0.9·L·(T + 5 ms)/n on oracle43's c2 SH windows (G=1.1 rows; T = rtprop+S live,")
    print("  i.e. with the anchor as it actually stood), by phase:")
    for sd, rp in (("42", 1), ("42", 2), ("7", 1)):
        txt = gshow(root, f"{base}c2-100-SH-s{sd}-r{rp}-a1-c.log")
        ws = [(float(g[0]), int(g[1]), int(g[3]) + int(g[6])) for g in OC.findall(txt) if float(g[0]) >= 1.0]
        def ratio(sel):
            v = [0.9 * L2 * (T / 1e6 + 5e-3) / n for t, n, T in sel]
            return f"median {st.median(v):.3f} max {max(v):.3f} (n {min(x[1] for x in sel)}-{max(x[1] for x in sel)}, {len(v)} win)" if v else "-"
        hold = [x for x in ws if 13.0 <= x[0] < 15.0]
        post = [x for x in ws if x[0] >= 15.0]
        print(f"    SH s{sd} r{rp}: hold's last 2 s {ratio(hold)} | after release {ratio(post)}")
    print("  The offline expired map (s43tau, G=0.9 τ=5) has no upward crossing; ask/n 0.87 at 1 024. With ask/n < 1 at every")
    print("  SH window, start-high descends; on the way down the anchor refreshes at the low point (brake ≈ 0 there, §42),")
    print("  so it settles where start-low does, unless the closed loop holds a crossing between 288 and 1 500 the ledgers")
    print("  do not sample (no oracle43 c2 window there outside transients).")
    return a0, b


def part2_3(root):
    print("\n#### 2. τ = b0 + c: c window per cell (window − b0, ms)")
    c2, c8 = w.load("p27", "c2-100"), w.load("p27", "c8-100")
    c1 = w.load("p34", "c1s-400", drop=True)
    win = defaultdict(dict)
    b0s = {}
    for c, caps in (("c2-100", c2), ("c8-100", c8), ("c1s-400", c1)):
        L = [x / s.W_WIRE for x in s.PATH_LINK[c]]
        b0 = [st.mean(caps[n]["legs"][p]["tm"] - caps[n]["legs"][p]["rp"] for n in w.SMALL[c]) for p in range(len(L))]
        b0s[c] = sum(L[p] * b0[p] for p in range(len(L))) / sum(L) / 1e3
        for G in w.GS:
            import io, contextlib
            with contextlib.redirect_stdout(io.StringIO()):
                lo, trap, band = w.window_static("", c, caps, G, L, False)
            hi = min(trap, band)
            if c == "c1s-400":
                th = w.c1s_oracle_quiet(root, G, L)
                knee = w.KNEE[c]
                otrap = min(v[1] for v in th.values() if v[0] > knee)
                yx = {v[0]: v[0] / (G * sum(L)) * 1e3 - v[1] for v in th.values()}
                yf = {v[0]: v[0] / (G * sum(L)) * 1e3 - v[2] for v in th.values()}
                e = 1.5 * knee
                oband = min(e / (G * sum(L)) * 1e3 - interp(yx, e), e / (G * sum(L)) * 1e3 - interp(yf, e))
                hi = min(otrap, oband)
            win[c][G] = (lo, hi)
    for G in w.GS:
        parts, lo_all, hi_all = [], -math.inf, math.inf
        for c in ("c2-100", "c8-100", "c1s-400"):
            lo, hi = win[c][G]
            a, bb = lo - b0s[c], hi - b0s[c]
            parts.append(f"{c.split('-')[0]} (b0 {b0s[c]:.2f}) [{a:.2f}, {bb:.2f})")
            lo_all, hi_all = max(lo_all, a), min(hi_all, bb)
        print(f"  G={G:4.2f}: " + "; ".join(parts) + "  => common c: " + (f"[{lo_all:.2f}, {hi_all:.2f})" if hi_all > lo_all else "NONE")
              + "   (const τ for comparison: " + (lambda l, h: f"[{l:.2f}, {h:.2f})" if h > l else "NONE")(
                  max(win[c][G][0] for c in win), min(win[c][G][1] for c in win)) + ")")

    print("\n#### 3. c1s at G = 0.9: settling points for τ = 3.5 and 5 ms (crossings of θ(n) = τ; θ < τ means ask > n)")
    L = [x / s.W_WIRE for x in s.PATH_LINK["c1s-400"]]
    th = w.c1s_oracle_quiet(root, 0.9, L)
    pts = sorted(th.values())
    print("  oracle43 bins (median n, θ_exp, θ_fresh): " + ", ".join(f"({n:.0f}, {x:.2f}, {f:.2f})" for n, x, f in pts))
    stat = {n: (w.theta(c1, n, L, 0.9, "expired"), w.theta(c1, n, L, 0.9, "fresh")) for n in sorted(c1)}
    print("  p34 static caps (n, θ_exp, θ_fresh; truncated minima): " + ", ".join(f"({n}, {x:.2f}, {f:.2f})" for n, (x, f) in stat.items()))

    def cross(seq, tau):
        out = []
        for (n0, t0), (n1, t1) in zip(seq, seq[1:]):
            if (t0 - tau) * (t1 - tau) < 0:
                x = n0 + (tau - t0) / (t1 - t0) * (n1 - n0)
                out.append(f"{x:.0f} ({'stable' if t1 > t0 else 'unstable'})")
        return out

    for tau in (3.5, 5.0):
        for lab, seqx, seqf in (("oracle43", [(n, x) for n, x, f in pts], [(n, f) for n, x, f in pts]),
                                ("p34", [(n, v[0]) for n, v in stat.items()], [(n, v[1]) for n, v in stat.items()])):
            print(f"  τ={tau} {lab:8s}: fresh map crossings {cross(seqf, tau) or 'none'}; expired map crossings "
                  f"{cross(seqx, tau) or 'none'}; top-cap θ_exp {seqx[-1][1]:.2f} ({'contracts' if seqx[-1][1] > tau else 'ask >= n'})")
    print("  Reading: start-low settles on the fresh-map crossing; start-high (from 2 048, anchor expired) on the expired-map")
    print("  crossing. oracle43 has no window below n ≈ 490, so the lower part of either map is p34's.")
    return b0s


def part4(root, b0s):
    print("\n#### 4. qsp_us against b0")
    print("  qsp_us = P90 − P50 (nearest rank) of the path's last L = 256 RTT samples, read at the [DIAG] tick:")
    print("    src/scheduler/path.rs:821-823 cand_quantile = monitor::quantile::nearest_rank; :848-855 rtt_qspread_us =")
    print("    cand_quantile(sorted rtt_win, 0.90) − cand_quantile(.., 0.50); :867-869 count = rtt_win.len();")
    print("    src/scheduler/copa.rs:190 SIGMA_CAND_WINDOW = 256 (the rtt_win ring, :1003 pops the oldest);")
    print("    src/net/diag.rs:679-680 reads it, :724 prints `qsp_us=<µs>/n<fill>` per path. No consumer.")
    print("  b0 = Tmin(250 ms window) − RTprop(10 s min) at the smallest caps: a different statistic (a min-to-min gap, not a")
    print("  P90−P50 spread). Per-row median over [DIAG] ticks t >= 1 s with a full window (n = 256), mean over rows:")
    for led, c, caps in (("p27", "c2-100", (64, 96)), ("p27", "c8-100", (128, 192)), ("p34", "c1s-400", (256, 384))):
        rows = [x for x in s.rows(led) if x["cell"] == c and x["arm"].startswith("DEF") and x["status"] == "LIVE"]
        for n in caps:
            per = defaultdict(list)
            for x in rows:
                if int(x["arm"][3:]) != n:
                    continue
                f = os.path.join(s.BASE, s.LEDGER[led], "diag-ld", f"{c}-{x['arm']}-s{x['seed']}-r{x['rep']}-a1-c.log")
                vals = defaultdict(list)
                for l in open(f, errors="replace"):
                    m = DGT.search(l)
                    if not m or float(m.group(1)) < 1.0:
                        continue
                    for g in QSP.finditer(l):
                        if int(g.group(3)) == 256:
                            vals[int(g.group(1))].append(int(g.group(2)))
                for p, v in vals.items():
                    per[p].append(st.median(v))
            print(f"    {led} {c} cap {n}: " + ", ".join(f"p{p} qsp {st.mean(v) / 1e3:.2f} ms (rows {len(v)})" for p, v in sorted(per.items())))
        print(f"    {c}: b0 (L-weighted) {b0s[c]:.2f} ms")
    base = "raptorpath/docs/l1-raw/oracle43/diag-ld/"
    for c, arm in (("c2-100", "SL"), ("c2-100", "SL-B0"), ("c1s-400", "SL")):
        v = []
        for sd, rp in (("42", 1), ("7", 1), ("42", 2)):
            if c == "c1s-400" and (sd, rp) == ("42", 2):
                continue
            txt = gshow(root, f"{base}{c}-{arm}-s{sd}-r{rp}-a1-c.log")
            q = []
            for l in txt.splitlines():
                m = DGT.search(l)
                if m and float(m.group(1)) >= 1.0:
                    for g in QSP.finditer(l):
                        if g.group(1) == "0" and int(g.group(3)) == 256:
                            q.append(int(g.group(2)))
            if q:
                v.append(st.median(q))
        print(f"    oracle43 {c} {arm} (closed loop, uncapped era): qsp row medians " + ", ".join(f"{x / 1e3:.2f}" for x in v) + " ms")


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    s.BASE, root = sys.argv[1], sys.argv[2]
    print(__doc__)
    part1(root)
    b0s = part2_3(root)
    part4(root, b0s)


if __name__ == "__main__":
    main()
