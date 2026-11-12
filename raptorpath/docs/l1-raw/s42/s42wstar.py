"""Status §42 Next-1, part 2: W*, dead band and closed-loop jitter per cell at the caps around each fixed point.
Uses s42noise.analyse (same window rule). Usage: python -I s42wstar.py <loop-diag dir>"""
import math, os, statistics as st, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import s42noise as s

Z90 = 1.2816


def phi(x):
    return 0.5 * (1 + math.erf(x / math.sqrt(2)))


def gamma_fit(vW):
    pts = [(math.log(W), math.log(v)) for W, v in vW.items() if v and v > 0 and not math.isnan(v)]
    if len(pts) < 3:
        return None
    mx = st.mean(p[0] for p in pts); my = st.mean(p[1] for p in pts)
    return -sum((x - mx) * (y - my) for x, y in pts) / sum((x - mx) ** 2 for x, _ in pts)


def emp_wstar(d, n, maxW=12):
    """smallest W whose non-overlapping AND moving block means both agree >= 0.9 (pooled over rows)."""
    for W in range(1, maxW + 1):
        kb = mb = km = mm = 0
        for sr in d["rowseries"]:
            a, b = s.agree(s.blocks(sr, W), n, d["sgn"]); kb += a; mb += b
            a, b = s.agree(s.mov(sr, W), n, d["sgn"]); km += a; mm += b
        if mb >= 3 and kb / mb >= 0.9 and km / mm >= 0.9:
            return W, kb, mb, km, mm
    return None


def model_wstar(d, n):
    """e = mu + b_row + eps; var(W) = sb2 + v1 W^-gamma. mu = mean pooled e (sign of map)."""
    es = [[math.log(x / n) for x in sr] for sr in d["rowseries"]]
    mu = st.mean(x for e in es for x in e)
    v1 = d["vW"][1]
    g = gamma_fit(d["vW"])
    rm = [st.mean(e) for e in es]
    sb2 = 0.0
    if len(rm) >= 2:
        sb2 = max(0.0, st.variance(rm) - v1 / st.mean(len(e) for e in es))
    if g is None or g <= 0:
        g = 1.0
    if mu * d["sgn"] <= 0:
        return mu, sb2, g, math.inf
    if sb2 > 0 and phi(abs(mu) / math.sqrt(sb2)) < 0.9:
        return mu, sb2, g, math.inf
    rhs = (abs(mu) / Z90) ** 2 - sb2
    W = (v1 / rhs) ** (1.0 / g) if rhs > 0 else math.inf
    return mu, sb2, g, max(1.0, W)


def main():
    cases = [("p27", "c2-100", [96, 128, 192, 256]),
             ("p27", "c8-100", [128, 192, 384, 512]),
             ("p34", "c1s-400", [384, 512, 768, 1024]),
             ("p27", "c1s-400", [384, 512, 768, 1024])]
    for led, cell, caps in cases:
        grid, amap, (kind, nfp, slope), res, live = s.analyse(led, cell)
        tr = (s.TRANSFER_S34 if led == "p34" else s.TRANSFER_S)[cell]
        print(f"\n## {led} {cell}: fixed point {nfp:.0f}, slope {slope:.2f}; transfer ref {tr} s")
        sds = []
        for n in caps:
            d = res.get(n)
            if not d:
                print(f"  {n}: no live rows"); continue
            mu, sb2, g, Wm = model_wstar(d, n)
            ew = emp_wstar(d, n)
            secs = [x for x in d["durs"] if x]
            third_row = st.median(secs) / 3
            sd_w = math.sqrt(d["vW"][1])
            lo_b = max(c for c in caps if c < nfp); hi_b = min(c for c in caps if c > nfp)
            if n in (lo_b, hi_b):
                sds.append(math.sqrt(d["vW"][1] + sb2))
            k1, m1 = 0, 0
            kc = mc = 0       # D's statistic: consecutive windows with the same sign(ask_w - n)
            for sr in d["rowseries"]:
                a, b = s.agree(sr, n, d["sgn"]); k1 += a; m1 += b
                sg = [1 if x > n else -1 for x in sr]
                kc += sum(1 for u, v in zip(sg, sg[1:]) if u == v); mc += len(sg) - 1
            def dec(W):
                if W is None or math.isinf(W):
                    return "inf", "0"
                Wc = math.ceil(W - 1e-9)
                return f"{Wc * 0.25:.2f}s", f"{tr / 3 / (Wc * 0.25):.1f} (row {third_row / (Wc * 0.25):.1f})"
            md = dec(Wm)
            ed = dec(ew[0] if ew else None)
            print(f"  n={n:5d} step={'+' if n > nfp else '-'}{abs(math.log(n / nfp)):.2f}(ln) rows={len(d['rowseries'])} win={m1}"
                  f" W1 agree {k1}/{m1}={k1 / m1:.2f} consec-same-sign {kc}/{mc}={kc / max(mc, 1):.2f} | mean e {mu:+.3f} sd_within {sd_w:.3f} sd_between {math.sqrt(sb2):.3f} gamma {g:.2f}"
                  f" | W*_model {Wm if math.isinf(Wm) else round(Wm, 1)} -> {md[0]}, dec/third {md[1]}"
                  f" | W*_emp {ew[0] if ew else '>12'} -> {ed[0]}, dec/third {ed[1]}")
        sd = st.mean(sds)   # bracket caps only, within + between-row sd of e at W=1
        half = Z90 * sd / (1 - slope)
        print(f"  bracket-cap sd(e) {sd:.3f}; dead band (single-window sign < 0.9, linearised map): |ln(n/n_fp)| < {half:.2f}"
              f" -> [{nfp * math.exp(-half):.0f}, {nfp * math.exp(half):.0f}]"
              f"; OPEN-LOOP PREDICTION for a closed loop n<-ask (AR1, white): sd ln n = {sd / math.sqrt(1 - slope ** 2):.3f}")


if __name__ == "__main__":
    main()
