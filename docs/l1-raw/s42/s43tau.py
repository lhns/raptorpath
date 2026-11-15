"""Status §43 follow-up: ask = G·ΣLᵢ(RTpropᵢ + Sᵢ + τ), S = Tp50 − Tmin, G ∈ {0.85..1.0}, τ ∈ {0..10} ms, p27 c2 and c8.
Fresh anchor = Copa rtprop_us row median; expired = row lowest post-warm-up rtt_min_us (as s42rtprop / s43gain).
OK = stable in-band point on BOTH maps and no upward crossing on the expired map inside the grid.
Bias note: §43's closed loop read the registered offset arm's point ~20 % above the offline map (233.5 vs 195), so
each offline point is also printed ×1.2 ("CL~"), with that value's band too. c2's grid ends at 1 024: no ask/n at 2 048.
Usage: python -I s43tau.py <loop-diag dir>"""
import os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import s42noise as s
from s43res import crossings, interp
from s43gain import legs_by_cap, KNEE

GS = (0.85, 0.9, 0.95, 1.0)
TAUS = (0.0, 2.5, 5.0, 7.5, 10.0)
CL_BIAS = 233.5 / 195


def cell(led, c):
    caps = legs_by_cap(led, c)
    grid = sorted(caps)
    L = [lb / s.W_WIRE for lb in s.PATH_LINK[c]]
    knee = KNEE[c]
    lo, hi = knee / 1.5, knee * 1.5
    gp = {n: caps[n]["mbps"] for n in grid}
    tp = {n: caps[n]["tp50"] for n in grid}
    gmax = max(gp.values())
    print(f"\n== {led} {c}: knee {knee}, band [{lo:.0f}, {hi:.0f}], grid {grid}" + ("  [THIN: 1 row at most caps]" if c == "c8-100" else ""))
    print(f"  {'G':>4} {'τms':>4} | {'fresh: fp slope band G/Gmax Tp50':34s} | {'expired: fp slope band G/Gmax Tp50':34s} | "
          f"{'expired up-crossings':22s} {'a/n@1024':>8} {'a/n@2048':>8} | CL~ fresh/expired (band) | OK")
    oks = []
    for G in GS:
        for tau in TAUS:
            res, ups, an = {}, None, {}
            for anchor in ("fresh", "expired"):
                a = {n: G * sum(L[p] * ((lg["rp"] if anchor == "fresh" else lg["fl"]) + lg["tp"] - lg["tm"] + tau * 1e3) / 1e6
                                for p, lg in enumerate(caps[n]["legs"])) for n in grid}
                xs = crossings(a)
                down = [x for x in xs if x[0] == "down"]
                if down:
                    _, x, sl, *_ = down[0]
                    res[anchor] = (x, sl, lo <= x <= hi, interp(gp, x) / gmax, interp(tp, x))
                else:
                    res[anchor] = None
                if anchor == "expired":
                    ups = [x[1] for x in xs if x[0] == "up"]
                    an = {n: a[n] / n for n in (1024, 2048) if n in a}

            def f(rr):
                if rr is None:
                    return f"{'none':34s}"
                x, sl, b, gf, t = rr
                return f"{x:5.0f} {sl:5.2f} {'yes' if b else 'no ':3s} {gf:6.3f} {t:5.1f}ms        "[:34]

            ok = all(res[k] and res[k][2] for k in res) and not ups
            cl = " / ".join(f"{res[k][0] * CL_BIAS:.0f}({'y' if res[k] and lo <= res[k][0] * CL_BIAS <= hi else 'n'})"
                            if res[k] else "-" for k in ("fresh", "expired"))
            print(f"  {G:4.2f} {tau:4.1f} | {f(res['fresh'])} | {f(res['expired'])} | "
                  f"{(', '.join(f'{u:.0f}' for u in ups) or 'none'):22s} {an.get(1024, float('nan')):8.3f} "
                  f"{an.get(2048, float('nan')):8.3f} | {cl:24s} | {'OK' if ok else ''}")
            if ok:
                oks.append((G, tau))
    print("  OK pairs (stable in-band on both maps, no upward crossing on the expired map in the grid): "
          + (", ".join(f"G={g} τ={t}" for g, t in oks) or "none"))


if __name__ == "__main__":
    sys.stdout.reconfigure(encoding="utf-8")
    s.BASE = sys.argv[1]
    print(__doc__)
    cell("p27", "c2-100")
    cell("p27", "c8-100")
