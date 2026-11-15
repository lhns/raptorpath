"""Status §42 follow-up: the row-median map with RTprop := the row's lowest post-warm-up rtt_min_us per leg
(what an expired 10 s min filter holds under a standing queue; pessimistic for a 10 s window). S unchanged
(row medians of Tp50 - Tmin, as ask27). Usage: python -I s42rtprop.py <loop-diag dir>"""
import os, re, statistics as st, sys
from collections import defaultdict
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import s42noise as s

LD = re.compile(r"\[LOOPDIAG\] t=([\d.]+)s p(\d+) n=(\d+) ov=\d+ rtt_min_us=(\S+)")


def row_tmin_floor(path, nleg):
    lo = defaultdict(lambda: None)
    for l in open(path, errors="replace"):
        m = LD.search(l)
        if not m or float(m.group(1)) < 1.0 or int(m.group(3)) == 0 or m.group(4) == "-":
            continue
        p, v = int(m.group(2)), float(m.group(4))
        lo[p] = v if lo[p] is None else min(lo[p], v)
    return [lo[i] for i in range(nleg)]


def mean(v):
    v = [x for x in v if x is not None]
    return st.mean(v) if v else None


def cell(led, c):
    rows = [r for r in s.rows(led) if r["cell"] == c and r["arm"].startswith("DEF") and r["status"] == "LIVE"]
    links = s.PATH_LINK[c]
    by_n = defaultdict(list)
    for r in rows:
        by_n[int(r["arm"][3:])].append(r)
    orig, new, info = {}, {}, {}
    for n in s.GRID[led][c]:
        rs = by_n.get(n)
        if not rs:
            continue
        floors = [row_tmin_floor(os.path.join(s.BASE, s.LEDGER[led], "diag-ld",
                  f"{c}-{r['arm']}-s{r['seed']}-r{r['rep']}-a1-c.log"), len(links)) for r in rs]
        to = tn = 0.0
        legs = []
        for p, lb in enumerate(links):
            g = lambda k: mean([r.get(f"ld_p{p}", {}).get(k) for r in rs])
            rp, tp, tm = g("rtprop_us"), g("rtt_p50_us"), g("rtt_min_us")
            fl = mean([f[p] for f in floors])
            S = tp - tm
            to += lb / s.W_WIRE * (rp + S) / 1e6
            tn += lb / s.W_WIRE * (fl + S) / 1e6
            legs.append(f"p{p}: rtprop {rp / 1e3:.2f} Tmin_floor {fl / 1e3:.2f} Tmin_med {tm / 1e3:.2f} "
                        f"floor-rtprop {(fl - rp) / 1e3:+.2f} med-rtprop {(tm - rp) / 1e3:+.2f} S {S / 1e3:.2f} ms")
        orig[n], new[n] = s.QG * to, s.QG * tn
        info[n] = (len(rs), legs)
    print(f"\n== {led} {c} ==")
    print(f"  {'n':>5} {'rows':>4} {'ask orig':>8} {'ask new':>8}  per leg (ms)")
    for n in sorted(orig):
        print(f"  {n:5d} {info[n][0]:4d} {orig[n]:8.0f} {new[n]:8.0f}  " + " | ".join(info[n][1]))
    for lab, a in (("orig", orig), ("new", new)):
        k, fp, sl = s.fixed_point(a)
        ns = sorted(a)
        # caps above the first downward crossing where ask climbs back to >= n
        back = []
        if fp is not None:
            back = [n for n in ns if n > fp and a[n] >= n]
        xs = []
        for lo, hi in zip(ns, ns[1:]):
            flo, fhi = a[lo] - lo, a[hi] - hi
            if (flo >= 0) != (fhi >= 0):
                x = lo + flo / (flo - fhi) * (hi - lo)
                xs.append(f"{'down(stable)' if flo >= 0 else 'up(unstable)'} {x:.0f} slope {(a[hi] - a[lo]) / (hi - lo):.2f}")
        print(f"  {lab}: classifier {k} n_fp={fp and round(fp)} slope={sl and round(sl, 3)}; ask>=n again above fp at: {back or 'none'}")
        print(f"        all crossings: {'; '.join(xs) or 'none'}")


if __name__ == "__main__":
    s.BASE = sys.argv[1]
    for led, c in (("p27", "c2-100"), ("p27", "c8-100"), ("p34", "c1s-400"), ("p27", "c1s-400")):
        cell(led, c)
