"""Status §34 (b1/b2): quinn's BBR window read as the knee, and the cwnd-binding share.

Per 250 ms [DIAG] window (t >= 1 s): K = sum over paths of qcwnd (quinn's
congestion window, bytes) / WIRE, in datagrams (one symbol per datagram); the
store cap c = the win= denominator (symbols). Per row: the median K; the
authority share = fraction of windows with K < c (quinn's window, not the store
cap, is the smaller bound; an end-of-window snapshot, as §30's checks.py);
cv(K) = sd/mean of K over the row's windows (noise of the reading at a fixed cap).
Per (ledger, cell, arm): means over rows. Static-cap arms (§27 DEF<n>, §29 KNEE)
give K(n) on the cap grid, so Spearman(K, n) is the circularity test (§27's Q1
rule shape), and the ask (1 + q)*K is compared with the static knee band.
Usage: python -I qcwnd.py <glob> ...   (client logs)"""
import glob, math, os, re, sys, statistics as st
from collections import defaultdict

WIRE = 1279  # wire bytes per datagram (status §24/§27 readouts)
Q = 0.1      # Bulk
DIAG = re.compile(r"^\[DIAG\] t=([\d.]+)s win=(\d+)/(\d+) ")
QC = re.compile(r"qcwnd=(\d+)")


def rows(f):
    out = []
    for line in open(f, errors="replace"):
        m = DIAG.match(line)
        if not m or float(m.group(1)) < 1.0:
            continue
        qs = [int(x) for x in QC.findall(line)]
        if not qs:
            continue
        out.append((int(m.group(3)), sum(qs) / WIRE))
    return out


def spearman(x, y):
    def rank(v):
        o = sorted(range(len(v)), key=lambda i: v[i])
        r = [0.0] * len(v)
        for k, i in enumerate(o):
            r[i] = k
        return r
    rx, ry = rank(x), rank(y)
    mx, my = st.mean(rx), st.mean(ry)
    num = sum((a - mx) * (b - my) for a, b in zip(rx, ry))
    den = math.sqrt(sum((a - mx) ** 2 for a in rx) * sum((b - my) ** 2 for b in ry))
    return num / den if den else float("nan")


def main():
    res = defaultdict(list)
    for pat in sys.argv[1:]:
        for f in sorted(glob.glob(pat)):
            name = os.path.basename(f)[:-6]
            m = re.match(r"(c\w+-\d+)-([A-Z]+\d*)-s(\d+)-r(\d+)", name)
            if not m:
                continue
            ledger = os.path.basename(os.path.dirname(os.path.dirname(f)))
            ws = rows(f)
            if len(ws) < 4:
                continue
            ks = [k for _, k in ws]
            res[(ledger, m.group(1), m.group(2))].append(dict(
                cap=st.median(c for c, _ in ws), K=st.median(ks),
                share=sum(1 for c, k in ws if k < c) / len(ws),
                cv=st.pstdev(ks) / st.mean(ks)))
    print("ledger    cell     arm      rows  cap_med  K_med(dgrams) (1+q)K  share_K<cap  cv(K)")
    grid = defaultdict(list)
    for k in sorted(res):
        v = res[k]
        cap = st.median(x["cap"] for x in v)
        K = st.mean(x["K"] for x in v)
        print(f"{k[0]:9s} {k[1]:8s} {k[2]:8s} {len(v):4d} {cap:8.0f} {K:13.0f} {(1 + Q) * K:7.0f} "
              f"{st.mean(x['share'] for x in v):11.2f} {st.mean(x['cv'] for x in v):6.2f}")
        if k[2].startswith("DEF"):
            grid[(k[0], k[1])].append((cap, K))
    for k, pts in sorted(grid.items()):
        pts.sort()
        print(f"SPEARMAN {k[0]} {k[1]} n={len(pts)} rho(K, cap)={spearman([p[0] for p in pts], [p[1] for p in pts]):.2f} "
              f"K range {min(p[1] for p in pts):.0f}-{max(p[1] for p in pts):.0f}")


if __name__ == "__main__":
    main()
