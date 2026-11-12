#!/usr/bin/env python3
"""§36 amendment 7: the named byte-floored re-score on the kept per-skb tables (offline).

From each anchor j the train extends until the bytes after j reach 4 x the row's
byte-weighted median skb size; rate = those bytes / span. Per 250 ms window the max
(scored form) and the median are taken; per row the median over windows and the cv of
the window maxima. Classes as D1/D2 (disp36.d1_class / d2_class). Reported beside the
scored k = 4 result; a verdict from it is a new registration.
usage: disp36_rescore7.py <tables dir>
"""
import gzip, math, os, re, sys
import disp36 as D

LINKS = {"c1s-400": [125e6], "c1d-400": [125e6, 125e6], "c2-100": [12.5e6], "c8-100": [12.5e6, 2.5e6]}
NAME = re.compile(r"^(c\w+-\d+)-CAP(\d+)-s(\d+)-r\d+\.p(\d)\.gz$")


def load(path):
    out, t = [], None
    with gzip.open(path, "rt") as fh:
        for line in fh:
            a, b = line.split()
            t = int(a) if t is None else t + int(a)
            out.append((t, int(b)))
    return out


def bw_median(bs):
    s = sorted(bs)
    half, acc = sum(s) / 2, 0
    for b in s:
        acc += b
        if acc >= half:
            return b
    return s[-1]


def estimate(skbs):
    if len(skbs) < 2:
        return None
    floor = 4 * bw_median([b for _, b in skbs])
    t0, tl = skbs[0][0] + D.WARM_NS, skbs[-1][0]
    n = len(skbs)
    pre = [0]
    for _, b in skbs:
        pre.append(pre[-1] + b)
    wins, i = {}, 1
    for j in range(n):
        # smallest i > j with bytes of skbs j+1..i >= floor (pre[i+1] - pre[j+1])
        i = max(i, j + 1)
        while i < n and pre[i + 1] - pre[j + 1] < floor:
            i += 1
        if i >= n:
            break
        tj = skbs[j][0]
        dt = skbs[i][0] - tj
        if tj >= t0 and dt > 0:
            wins.setdefault((tj - t0) // D.WIN_NS, []).append((pre[i + 1] - pre[j + 1]) * D.NS / dt)
    keep = [w for w in sorted(wins) if t0 + (w + 1) * D.WIN_NS <= tl]
    if not keep:
        return None
    mx = [max(wins[w]) for w in keep]
    wm = [D._median(wins[w]) for w in keep]
    mean = sum(mx) / len(mx)
    cv = math.sqrt(sum((x - mean) ** 2 for x in mx) / len(mx)) / mean
    return D._median(mx), D._median(wm), cv


def main(tdir):
    acc = {}
    for f in sorted(os.listdir(tdir)):
        m = NAME.match(f)
        if not m:
            continue
        cell, n, leg = m.group(1), int(m.group(2)), int(m.group(4))
        skbs = load(os.path.join(tdir, f))
        if len(skbs) < 200:
            continue
        e = estimate(skbs)
        if e:
            acc.setdefault((cell, leg), {}).setdefault(n, []).append(e)
    print("§36 AMENDMENT-7 RE-SCORE (byte-floored trains, floor = 4 x byte-weighted median skb); reported, not a verdict")
    for (cell, leg), byn in sorted(acc.items()):
        link = LINKS[cell][leg]
        q = {n: sum(e[0] for e in v) / len(v) / link for n, v in byn.items()}
        qm = {n: sum(e[1] for e in v) / len(v) / link for n, v in byn.items()}
        cv = {n: (sum(e[2] for e in v) / len(v), max(e[2] for e in v)) for n, v in byn.items()}
        print(f"CELL {cell} p{leg}")
        print("  n      rows  Q(max)  Q(wmed)  cv mean/max")
        for n in sorted(byn):
            print(f"  {n:<6} {len(byn[n]):<5} {q[n]:7.3f} {qm[n]:8.3f}  {cv[n][0]:.3f}/{cv[n][1]:.3f}")
        print(f"  D1(max) {D.d1_class(q, cell)}  D1(wmed) {D.d1_class(qm, cell)}  D2 {D.d2_class(cv, cell)}")


if __name__ == "__main__":
    main(sys.argv[1])
