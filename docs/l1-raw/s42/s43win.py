"""Status §43b pair rule + the τ window per cell (offline, read-only).

Law: ask = G·ΣLᵢ(RTpropᵢ + Sᵢ + τ), S = Tp50 − Tmin. Anchors: fresh = Copa rtprop_us; expired = the row's lowest
post-warm-up Tmin (s42rtprop). Per cap the map is EXACT in τ, so ask(n) < n  <=>  τ < θ(n) with
    θ_A(n) = n/(G·ΣL) − Σ(Lᵢ/ΣL)·(anchor_A,ᵢ + Sᵢ)                   (A = fresh | expired)
  = β(n) + (1 − G·L/D)·n/(G·L)  under Little (n ≈ D·Tp50, single leg), β = Tmin − expired anchor (printed beside it).
Window at gain G:
  τ_lo = max_A θ_A(n*/1.5)  (ask >= n at the band's lower edge on both maps: the stable point is >= n*/1.5)
  τ_trap = min over grid caps n > n* of θ_exp(n)  (no upward crossing on the expired map above the knee, in the grid)
  τ_band = min_A θ_A(1.5·n*)  (stable point <= 1.5·n* on both maps)
  window = [τ_lo, min(τ_trap, τ_band)). θ at non-grid n is linear interpolation of its anchor+S term.
b₀ = mean (Tmin − RTprop_fresh) over the two smallest caps (c2 64/96, c8 128/192, c1s p34 256/384 ex-outlier).
c1s: p34 static rows (Tmin/Tp50 TRUNCATED to the first 8 192 samples per window; DEF384 s42r1 outlier dropped), and
oracle43's uncapped [OCAP] windows (github/harness43) binned by n, for θ near and above the knee only (no low caps:
the closed loop never sat below ~490). Usage: python -I s43win.py <loop-diag dir> <worktree root>"""
import math, os, re, statistics as st, subprocess, sys
from collections import defaultdict
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import s42noise as s
import s42rtprop as r
from s43res import crossings, interp

GS = (0.85, 0.9, 0.95, 1.0)
TAUS = (0.0, 2.5, 5.0, 7.5, 10.0)
KNEE = {"c2-100": 192, "c8-100": 384, "c1s-400": 1024}
SMALL = {"c2-100": (64, 96), "c8-100": (128, 192), "c1s-400": (256, 384)}
CLB = 233.5 / 195


def load(led, c, drop=False):
    rows = [x for x in s.rows(led) if x["cell"] == c and x["arm"].startswith("DEF") and x["status"] == "LIVE"]
    by_n = defaultdict(list)
    for x in rows:
        by_n[int(x["arm"][3:])].append(x)
    if drop:
        for n, rs in by_n.items():
            by_n[n] = [x for x in rs if not (len(rs) > 1 and x["mbps"] < 0.6 * st.median(y["mbps"] for y in rs if y is not x))]
    caps = {}
    for n in s.GRID[led][c]:
        rs = by_n.get(n)
        if not rs:
            continue
        fl = [r.row_tmin_floor(os.path.join(s.BASE, s.LEDGER[led], "diag-ld",
              f"{c}-{x['arm']}-s{x['seed']}-r{x['rep']}-a1-c.log"), len(s.PATH_LINK[c])) for x in rs]
        legs = []
        for p in range(len(s.PATH_LINK[c])):
            g = lambda k: r.mean([x.get(f"ld_p{p}", {}).get(k) for x in rs])
            legs.append(dict(rp=g("rtprop_us"), tp=g("rtt_p50_us"), tm=g("rtt_min_us"), fl=r.mean([f[p] for f in fl]),
                             D=g("dlv_rate")))
        caps[n] = dict(legs=legs, mbps=st.mean(x["mbps"] for x in rs), tp50=legs[0]["tp"] / 1e3, rows=len(rs))
    return caps


def Y(caps, n, L, anchor):
    """L-weighted (anchor + S), µs, at a grid cap."""
    lg = caps[n]["legs"]
    return sum(L[p] * ((l["rp"] if anchor == "fresh" else l["fl"]) + l["tp"] - l["tm"]) for p, l in enumerate(lg)) / sum(L)


def theta(caps, n, L, G, anchor):
    """ms; interpolates Y between grid caps."""
    y = interp({m: Y(caps, m, L, anchor) for m in caps}, n)
    return (n / (G * sum(L)) * 1e6 - y) / 1e3


def wt(caps, n, L, key):
    return sum(L[p] * caps[n]["legs"][p][key] for p in range(len(L))) / sum(L)


# ---------------- part 1: §43b's rule on the s43tau sweep (recomputed, same maps as s43tau.py)
def sweep(caps, c, L):
    knee = KNEE[c]
    lo, hi = knee / 1.5, knee * 1.5
    grid = sorted(caps)
    gp = {n: caps[n]["mbps"] for n in grid}
    tp = {n: caps[n]["tp50"] for n in grid}
    gmax = max(gp.values())
    out = {}
    for G in GS:
        for tau in TAUS:
            res, ups = {}, []
            for A in ("fresh", "expired"):
                a = {n: G * sum(L) * (Y(caps, n, L, A) + tau * 1e3) / 1e6 for n in grid}
                xs = crossings(a)
                dn = [x for x in xs if x[0] == "down"]
                res[A] = (dn[0][1], dn[0][2], interp(gp, dn[0][1]) / gmax, interp(tp, dn[0][1])) if dn else None
                if A == "expired":
                    ups = [x[1] for x in xs if x[0] == "up"]
            ok = all(res[A] and lo <= res[A][0] <= hi for A in res) and not ups
            out[(G, tau)] = dict(res=res, ups=ups, ok=ok)
    return out


def part1(c2, c8):
    print("\n#### 1. §43b pair-selection rule (status §43b), applied to the s43tau sweep (recomputed here)")
    L2 = [x / s.W_WIRE for x in s.PATH_LINK["c2-100"]]
    L8 = [x / s.W_WIRE for x in s.PATH_LINK["c8-100"]]
    w2, w8 = sweep(c2, "c2-100", L2), sweep(c8, "c8-100", L8)
    print("  eligible = the sweep's OK pairs (stable in band on both maps, no expired upward crossing) at c2 AND c8")
    print(f"  {'G':>4} {'τ':>4} | {'okc2':4} {'okc8':4} | rule1 G·1.016·1.04<1 | ×1.2 c2 fresh/exp (<= 263?) | slope_exp c2 | c2 G/Gmax f/e")
    cand = []
    for (G, tau), d in w2.items():
        rf, rx = d["res"]["fresh"], d["res"]["expired"]
        r1 = G * 1.016 * 1.04 < 1
        r2 = rf and rx and rf[0] * CLB <= 263 and rx[0] * CLB <= 263
        elig = d["ok"] and w8[(G, tau)]["ok"]
        print(f"  {G:4.2f} {tau:4.1f} | {'y' if d['ok'] else 'n':4} {'y' if w8[(G, tau)]['ok'] else 'n':4} | "
              f"{'pass' if r1 else 'fail':19} | "
              + (f"{rf[0] * CLB:4.0f} / {rx[0] * CLB:4.0f} {'pass' if r2 else 'fail':4}" if rf and rx else "-            ")
              + (f"           | {rx[1]:5.2f}        | {rf[2]:.3f} / {rx[2]:.3f}" if rx and rf else ""))
        if elig and r1 and r2:
            cand.append((G, tau, rx[1], (rf[2] + rx[2]) / 2))
    if not cand:
        print("  no pair passes rules 1-2")
        return None
    smin = min(x[2] for x in cand)
    tie = [x for x in cand if x[2] <= smin + 0.1]
    best = max(tie, key=lambda x: x[3])
    print(f"  after rules 1-2: " + ", ".join(f"({g}, {t}) slope {sl:.2f}" for g, t, sl, _ in cand))
    print(f"  rule 3 (lowest expired slope {smin:.2f}, ties within 0.1): " + ", ".join(f"({g}, {t})" for g, t, *_ in tie))
    print(f"  rule 4 (highest mean of fresh/expired c2 G/Gmax): CHOSEN G = {best[0]}, τ = {best[1]} ms")
    d = w2[(best[0], best[1])]
    for A in ("fresh", "expired"):
        x, sl, gf, t = d["res"][A]
        print(f"    c2 {A:7s}: point {x:.0f} -> predicted band [{x:.0f}, {x * CLB:.0f}] (offline to ×1.2), slope {sl:.2f},"
              f" G/Gmax {gf:.3f}, Tp50 {t:.1f} ms")
    for A in ("fresh", "expired"):
        x, sl, gf, t = w8[(best[0], best[1])]["res"][A]
        print(f"    c8 {A:7s} (thin): point {x:.0f} -> [{x:.0f}, {x * CLB:.0f}], slope {sl:.2f}, G/Gmax {gf:.3f}, Tp50 {t:.1f} ms")
    # robustness: same rule without the c8 eligibility
    return best


# ---------------- part 2: the τ window
def window_static(label, c, caps, G, L, verbose):
    knee = KNEE[c]
    grid = sorted(caps)
    lo, hi = knee / 1.5, knee * 1.5
    t_lo = max(theta(caps, lo, L, G, A) for A in ("fresh", "expired"))
    above = [n for n in grid if n > knee]
    t_trap = min(theta(caps, n, L, G, "expired") for n in above) if above else math.inf
    t_band = min(theta(caps, hi, L, G, A) for A in ("fresh", "expired"))
    if verbose:
        print(f"   per cap (G={G}): n | β = Tmin−floor (ms, L-wt) | brake_fresh | θ_exp(n) | θ_fresh(n) | Little form β+(1−G·L/D)n/(G·L)")
        for n in grid:
            b = wt(caps, n, L, "tm") - wt(caps, n, L, "fl")
            bf = wt(caps, n, L, "tm") - wt(caps, n, L, "rp")
            D = sum(caps[n]["legs"][p]["D"] for p in range(len(L)))
            lit = (b / 1e3 + (1 - G * sum(L) / D) * n / (G * sum(L)) * 1e3)
            print(f"     {n:5d} | {b / 1e3:6.2f} | {bf / 1e3:6.2f} | {theta(caps, n, L, G, 'expired'):7.2f} | "
                  f"{theta(caps, n, L, G, 'fresh'):7.2f} | {lit:7.2f}" + ("   (above knee)" if n > knee else ""))
    return t_lo, t_trap, t_band


def ocap_c1s(root):
    """oracle43 c1s [OCAP] windows -> {row: [(n, rtprop, tmin, tp50)]}, floor per row; drops SL s42 r2 (quinn collapse)."""
    rx = re.compile(r"\[OCAP\] t=([\d.]+)s p0 law=\S+ n=(\d+) .*?rtprop_us=(\d+) tmin_us=(\d+) tp50_us=(\d+)")
    ls = subprocess.run(["git", "-C", root, "ls-tree", "-r", "--name-only", "github/harness43"],
                        capture_output=True, text=True).stdout.split()
    rows = {}
    for f in ls:
        m = re.search(r"oracle43/diag-ld/(c1s-400-(SL|SH|MAIN)-s\d+-r\d+)-a1-c\.log$", f)
        if not m or m.group(1) == "c1s-400-SL-s42-r2":
            continue
        txt = subprocess.run(["git", "-C", root, "show", f"github/harness43:{f}"], capture_output=True,
                             text=True, errors="replace").stdout
        ws = [(int(g[1]), int(g[2]), int(g[3]), int(g[4])) for g in rx.findall(txt) if float(g[0]) >= 1.0]
        if ws:
            rows[m.group(1)] = ws
    return rows


def c1s_oracle(root, G, L, b_lo_y):
    rows = ocap_c1s(root)
    bins = defaultdict(lambda: defaultdict(list))
    edges = [(448, 640, 512), (640, 896, 768), (896, 1280, 1024), (1280, 1792, 1536), (1792, 2049, 2048)]
    for name, ws in rows.items():
        fl = min(w[2] for w in ws)
        for n, rp, tm, tp in ws:
            for a, b, ctr in edges:
                if a <= n < b:
                    d = bins[ctr]
                    d["n"].append(n); d["yf"].append(rp + tp - tm); d["yx"].append(fl + tp - tm)
                    d["beta"].append(tm - fl); d["bf"].append(tm - rp); d["tp"].append(tp); d["row"].append(name)
    print(f"   oracle43 uncapped [OCAP] windows (t >= 1 s; SL s42 r2 = quinn collapse dropped; rows {sorted(rows)})")
    print(f"   bin | windows rows | median n | β=Tmin−floor | brake_fresh | Tp50 | θ_exp(n) | θ_fresh(n)   (G={G}, medians, ms)")
    th = {}
    for ctr in sorted(bins):
        d = bins[ctr]
        nm = st.median(d["n"])
        tx = (nm / (G * sum(L)) * 1e6 - st.median(d["yx"])) / 1e3
        tf = (nm / (G * sum(L)) * 1e6 - st.median(d["yf"])) / 1e3
        th[ctr] = (nm, tx, tf)
        print(f"   {ctr:5d} | {len(d['n']):4d} {len(set(d['row'])):2d} | {nm:6.0f} | {st.median(d['beta']) / 1e3:6.2f} | "
              f"{st.median(d['bf']) / 1e3:6.2f} | {st.median(d['tp']) / 1e3:6.2f} | {tx:7.2f} | {tf:7.2f}")
    return th


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    s.BASE, root = sys.argv[1], sys.argv[2]
    print(__doc__)
    c2, c8 = load("p27", "c2-100"), load("p27", "c8-100")
    c1 = load("p34", "c1s-400", drop=True)
    best = part1(c2, c8)

    print("\n#### 2. τ window per cell")
    cells = [("c2 (p27)", "c2-100", c2), ("c8 (p27, THIN)", "c8-100", c8), ("c1s (p34, TRUNCATED minima)", "c1s-400", c1)]
    info = {}
    for label, c, caps in cells:
        L = [x / s.W_WIRE for x in s.PATH_LINK[c]]
        b0 = [st.mean(caps[n]["legs"][p]["tm"] - caps[n]["legs"][p]["rp"] for n in SMALL[c]) for p in range(len(L))]
        b0w = sum(L[p] * b0[p] for p in range(len(L))) / sum(L)
        knee = KNEE[c]
        rpk = wt(caps, knee, L, "rp")
        print(f"\n  == {label}: knee {knee}, band [{knee / 1.5:.0f}, {knee * 1.5:.0f}]; b0 = "
              + ", ".join(f"p{p} {b / 1e3:.2f}" for p, b in enumerate(b0)) + f" ms (L-wt {b0w / 1e3:.2f}); "
              f"RTprop_fresh at the knee (L-wt) {rpk / 1e3:.2f} ms")
        info[c] = dict(b0=b0w / 1e3, rp=rpk / 1e3, win={})
        for G in GS:
            t_lo, t_trap, t_band = window_static(label, c, caps, G, L, verbose=(G == 0.9))
            hi_ = min(t_trap, t_band)
            info[c]["win"][G] = (t_lo, hi_)
            print(f"   G={G:4.2f}: τ_lo {t_lo:6.2f} | τ_trap {t_trap:6.2f} | τ_band {t_band:6.2f} -> window "
                  + (f"[{t_lo:.2f}, {hi_:.2f})" if hi_ > t_lo else f"EMPTY ({t_lo:.2f} >= {hi_:.2f})"))
        if c == "c1s-400":
            print("\n  == c1s from oracle43 (uncapped minima), near and above the knee")
            for G in GS:
                th = c1s_oracle(root, G, L, None) if G == 0.9 else None
                if th is None:
                    th = {k: v for k, v in c1s_oracle_quiet(root, G, L).items()}
                trap = min(v[1] for v in th.values() if v[0] > knee)          # bins whose median n is above the knee
                # band edge 1.5·n* = 1536: interpolate the anchor+S term Y = n/(G·L) − θ between bin medians
                yx = {v[0]: v[0] / (G * sum(L)) * 1e3 - v[1] for v in th.values()}
                yf = {v[0]: v[0] / (G * sum(L)) * 1e3 - v[2] for v in th.values()}
                e = 1.5 * knee
                band = min(e / (G * sum(L)) * 1e3 - interp(yx, e), e / (G * sum(L)) * 1e3 - interp(yf, e))
                print(f"   G={G:4.2f}: oracle43 τ_trap (bins > knee) {trap:6.2f} | τ_band (θ interpolated at n=1536) {band:6.2f}"
                      f" | with p34 τ_lo {info[c]['win'][G][0]:.2f} -> window "
                      + (f"[{info[c]['win'][G][0]:.2f}, {min(trap, band):.2f})" if min(trap, band) > info[c]['win'][G][0]
                         else "EMPTY"))
                info["c1s-oracle"] = info.get("c1s-oracle", dict(b0=info[c]["b0"], rp=info[c]["rp"], win={}))
                info["c1s-oracle"]["win"][G] = (info[c]["win"][G][0], min(trap, band))

    print("\n#### 3. One coefficient for every cell? (windows [τ_lo, τ_hi) in ms; forms τ = const, k·RTprop, k·b0)")
    keys = [("c2-100", "c2"), ("c8-100", "c8 (thin)"), ("c1s-400", "c1s p34"), ("c1s-oracle", "c1s p34 lo / oracle43 hi")]
    for G in GS:
        print(f"  G={G:4.2f}")
        for form, scale in (("const (ms)", lambda d: 1.0), ("k·RTprop", lambda d: d["rp"]), ("k·b0", lambda d: d["b0"])):
            parts, lo_all, hi_all = [], -math.inf, math.inf
            for k, lab in keys:
                d = info[k]
                lo_, hi_ = d["win"][G]
                sc = scale(d)
                a, b = lo_ / sc, hi_ / sc
                parts.append(f"{lab} [{a:.2f}, {b:.2f})")
                if k != "c1s-400":      # intersect over c2, c8, and c1s with the uncapped upper bound
                    lo_all, hi_all = max(lo_all, a), min(hi_all, b)
            res = f"[{lo_all:.2f}, {hi_all:.2f})" if hi_all > lo_all else "NONE"
            print(f"    {form:10s}: " + "; ".join(parts) + f"  => common (c2, c8, c1s-oracle): {res}")
            # without c8
            lo2 = max(info[k]["win"][G][0] / scale(info[k]) for k in ("c2-100", "c1s-oracle"))
            hi2 = min(info[k]["win"][G][1] / scale(info[k]) for k in ("c2-100", "c1s-oracle"))
            print(f"    {'':10s}  without c8: " + (f"[{lo2:.2f}, {hi2:.2f})" if hi2 > lo2 else "NONE"))
    if best:
        print(f"\nChosen §43b pair: G = {best[0]}, τ = {best[1]} ms")


_cache = {}


def c1s_oracle_quiet(root, G, L):
    import io, contextlib
    buf = io.StringIO()
    with contextlib.redirect_stdout(buf):
        th = c1s_oracle(root, G, L, None)
    return th


if __name__ == "__main__":
    main()
