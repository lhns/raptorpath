"""Status §43 follow-up: the REGISTERED law ask = G·ΣLᵢ(RTpropᵢ + Sᵢ [+ b0ᵢ]), S = Tp50 − Tmin, at c2 and c8 (p27).
(1) debiased brake: b0ᵢ = mean of (Tmin − RTprop_fresh) over the smallest caps (c2 64/96; c8 128/192 per leg), G = 1.1;
(2) no debias, G = 1.2. Fresh anchor = Copa rtprop_us row median; expired = row lowest post-warm-up rtt_min_us
(s42rtprop.row_tmin_floor). Row medians averaged over LIVE rows per cap (as ask27).
Usage: python -I s43gain.py <loop-diag dir>   (imports s42noise.py / s42rtprop.py from this directory)"""
import os, statistics as st, sys
from collections import defaultdict
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import s42noise as s
import s42rtprop as r
from s43res import crossings, interp

KNEE = {"c2-100": 192, "c8-100": 384}
SMALL = {"c2-100": (64, 96), "c8-100": (128, 192)}


def legs_by_cap(led, c):
    rows = [x for x in s.rows(led) if x["cell"] == c and x["arm"].startswith("DEF") and x["status"] == "LIVE"]
    by_n = defaultdict(list)
    for x in rows:
        by_n[int(x["arm"][3:])].append(x)
    out = {}
    for n in s.GRID[led][c]:
        rs = by_n.get(n)
        if not rs:
            continue
        fl = [r.row_tmin_floor(os.path.join(s.BASE, s.LEDGER[led], "diag-ld",
              f"{c}-{x['arm']}-s{x['seed']}-r{x['rep']}-a1-c.log"), len(s.PATH_LINK[c])) for x in rs]
        legs = []
        for p in range(len(s.PATH_LINK[c])):
            g = lambda k: r.mean([x.get(f"ld_p{p}", {}).get(k) for x in rs])
            legs.append(dict(rp=g("rtprop_us"), tp=g("rtt_p50_us"), tm=g("rtt_min_us"), fl=r.mean([f[p] for f in fl])))
        out[n] = dict(legs=legs, mbps=st.mean(x["mbps"] for x in rs), tp50=legs[0]["tp"] / 1e3, rows=len(rs))
    return out


def cell(led, c):
    caps = legs_by_cap(led, c)
    grid = sorted(caps)
    L = [lb / s.W_WIRE for lb in s.PATH_LINK[c]]
    knee = KNEE[c]
    b0 = [st.mean(caps[n]["legs"][p]["tm"] - caps[n]["legs"][p]["rp"] for n in SMALL[c]) for p in range(len(L))]
    gp = {n: caps[n]["mbps"] for n in grid}
    tp = {n: caps[n]["tp50"] for n in grid}
    gmax = max(gp.values())
    print(f"\n== {led} {c}: knee {knee}, band [{knee / 1.5:.0f}, {knee * 1.5:.0f}]; rows/cap "
          + ", ".join(f"{n}:{caps[n]['rows']}" for n in grid)
          + "; b0 (ms) " + ", ".join(f"p{p} {b / 1e3:.2f}" for p, b in enumerate(b0)) + (" [THIN: c8]" if c == "c8-100" else ""))
    for lab, G, deb in (("gate: registered G=1.1", 1.1, False), ("(1) debiased G=1.1", 1.1, True),
                        ("(2) registered G=1.2", 1.2, False)):
        for anchor in ("fresh", "expired"):
            a = {}
            for n in grid:
                tot = 0.0
                for p, lg in enumerate(caps[n]["legs"]):
                    anc = lg["rp"] if anchor == "fresh" else lg["fl"]
                    tot += L[p] * (anc + lg["tp"] - lg["tm"] + (b0[p] if deb else 0.0)) / 1e6
                a[n] = G * tot
            xs = crossings(a)
            down = [x for x in xs if x[0] == "down"]
            line = f"  {lab:24s} {anchor:7s} ask " + ", ".join(f"{n}:{a[n]:.0f}" for n in grid)
            print(line)
            if a[grid[0]] < grid[0]:
                print("      FLOOR at the lowest cap")
            if down:
                _, x, sl, *_ = down[0]
                G_ = interp(gp, x)
                print(f"      stable {x:.0f} slope {sl:.2f}; band {'yes' if knee / 1.5 <= x <= knee * 1.5 else 'no'};"
                      f" G/Gmax {G_ / gmax:.3f} ({G_:.1f} Mbit/s); Tp50 {interp(tp, x):.1f} ms")
            elif not xs:
                print("      no crossing: " + ("CEILING (ask >= n everywhere)" if a[grid[0]] >= grid[0] else "FLOOR"))
            print("      all crossings: " + "; ".join(f"{k} {x:.0f}" for k, x, *_ in xs))
            ups = [x for x in xs if x[0] == "up"]
            print("      unstable threshold: " + ("; ".join(f"{x:.0f} (slope {sl:.2f})" for _, x, sl, *_ in ups) or "none")
                  + " | ask/n at caps >= knee: " + ", ".join(f"{n}:{a[n] / n:.2f}" for n in grid if n >= knee))


if __name__ == "__main__":
    sys.stdout.reconfigure(encoding="utf-8")
    s.BASE = sys.argv[1]
    print(__doc__)
    cell("p27", "c2-100")
    cell("p27", "c8-100")
