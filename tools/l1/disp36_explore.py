#!/usr/bin/env python3
"""s36 result addendum (exploratory, post hoc on the seen ledger): per cell/leg/cap, k = 4 trains,
the per-window p90 and median with their within-row cv, the delivered rate D (post-warm-up bytes / time),
and Lhat/D. Also the c1 batch-release check: share of k=4 trains spanning < b/R (compressed) and whether
the skb clock conserves time (span of 64 skbs vs 64·b/R). usage: disp36_explore.py <tables dir>"""
import math, os, sys
import disp36 as D
from disp36_rescore7 import LINKS, NAME, load

def cv(xs):
    m = sum(xs) / len(xs)
    return math.sqrt(sum((x - m) ** 2 for x in xs) / len(xs)) / m if m > 0 else float("nan")

def row(skbs, link, k=4):
    t0, tl = skbs[0][0] + D.WARM_NS, skbs[-1][0]
    post = [(t, b) for t, b in skbs if t >= t0]
    dly = sum(b for _, b in post[1:]) * D.NS / (post[-1][0] - post[0][0])
    pre = [0]
    for _, b in skbs: pre.append(pre[-1] + b)
    wins, comp, ntr = {}, 0, 0
    for j in range(len(skbs) - k):
        if skbs[j][0] < t0: continue
        dt = skbs[j + k][0] - skbs[j][0]
        by = pre[j + k + 1] - pre[j + 1]
        ntr += 1
        if dt * link < 0.5 * by * D.NS: comp += 1
        if dt > 0: wins.setdefault((skbs[j][0] - t0) // D.WIN_NS, []).append(by * D.NS / dt)
    if not wins: return dict(D=dly,p90=None,cvp90=None,med=None,cvmed=None,comp=0,cons_min=None,cons_p10=None)
    keep = [w for w in sorted(wins) if t0 + (w + 1) * D.WIN_NS <= tl]
    p9 = [D._pct(wins[w], 0.9) for w in keep]; md = [D._median(wins[w]) for w in keep]
    # time conservation: blocks of 64 skbs, measured span / ideal span at R (busy-run bound)
    cons = []
    for j in range(0, len(post) - 64, 64):
        by = sum(b for _, b in post[j + 1:j + 65]); sp = post[j + 64][0] - post[j][0]
        if sp > 0: cons.append(sp * link / (by * D.NS))
    cons.sort()
    return dict(D=dly, p90=D._median(p9), cvp90=cv(p9), med=D._median(md), cvmed=cv(md),
                comp=comp / max(ntr, 1), cons_min=cons[0] if cons else None, cons_p10=cons[len(cons) // 10] if cons else None)

def main(tdir):
    acc = {}
    for f in sorted(os.listdir(tdir)):
        m = NAME.match(f)
        if not m: continue
        cell, n, leg = m.group(1), int(m.group(2)), int(m.group(4))
        sk = load(os.path.join(tdir, f))
        if len(sk) < 200: continue
        acc.setdefault((cell, leg), {}).setdefault(n, []).append(row(sk, LINKS[cell][leg]))
    print("s36 ADDENDUM EXPLORATION (post hoc; k = 4). Q = /link; LD = Lhat/D (D = delivered post-warm-up rate)")
    for (cell, leg), byn in sorted(acc.items()):
        L = LINKS[cell][leg]
        print(f"CELL {cell} p{leg}")
        print("  n     rows  D/link  Qp90  cv_p90  LDp90  Qmed  cv_med  LDmed  compressed  span/ideal min,p10")
        for n in sorted(byn):
            r = [x for x in byn[n] if x['p90'] and x['med'] and x['cons_min']]
            if not r: print(f"  {n:<5} no usable rows"); continue
            a = lambda key: sum(x[key] for x in r) / len(r)
            print(f"  {n:<5} {len(r):<4} {a('D')/L:6.3f} {a('p90')/L:6.3f} {a('cvp90'):6.3f} {a('p90')/a('D'):6.2f} "
                  f"{a('med')/L:6.3f} {a('cvmed'):6.3f} {a('med')/a('D'):6.2f}  {a('comp'):8.4f}   "
                  f"{min(x['cons_min'] for x in r):.3f},{a('cons_p10'):.3f}")

if __name__ == "__main__":
    main(sys.argv[1])
