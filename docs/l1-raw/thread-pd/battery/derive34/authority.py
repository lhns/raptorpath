"""Status §34 (b2): which brake holds authority, per static cap — the store cap or
quinn's window.

Per 250 ms [DIAG] window (t >= 1 s): c = store cap (win= denominator), u = store
occupancy (win= numerator), K = sum of qcwnd / WIRE (quinn's window, datagrams),
B = sum over paths of dgq<i>[hand - tx] (datagrams handed to quinn's datagram
queue and not yet transmitted: quinn's own backlog, which grows only when quinn's
window or pacer, not the store cap, holds the sender back).
Per (ledger, cell, arm): medians over rows of the row medians of u/c, K, B, and
the share of windows with K < c; goodput from the [DIAG] good= mean.
Usage: python -I authority.py <glob> ...   (client logs)"""
import glob, os, re, sys, statistics as st
from collections import defaultdict

WIRE = 1279
DIAG = re.compile(r"^\[DIAG\] t=([\d.]+)s win=(\d+)/(\d+) .*? good=([\d.]+)Mbit")
QC = re.compile(r"qcwnd=(\d+)")
DGQ = re.compile(r"dgq\d+\[hand=(\d+) tx=(\d+)")


def main():
    res = defaultdict(list)
    for pat in sys.argv[1:]:
        for f in sorted(glob.glob(pat)):
            m = re.match(r"(c\w+-\d+)-([A-Z]+\d*)-s(\d+)-r(\d+)", os.path.basename(f))
            if not m:
                continue
            ledger = os.path.basename(os.path.dirname(os.path.dirname(f)))
            ws = []
            for line in open(f, errors="replace"):
                d = DIAG.match(line)
                if not d or float(d.group(1)) < 1.0:
                    continue
                qs = [int(x) for x in QC.findall(line)]
                dq = DGQ.findall(line)
                if not qs or not dq:
                    continue
                u, c, g = int(d.group(2)), int(d.group(3)), float(d.group(4))
                ws.append((u / c, sum(qs) / WIRE, sum(int(h) - int(t) for h, t in dq), c, g))
            if len(ws) >= 4:
                res[(ledger, m.group(1), m.group(2))].append(dict(
                    occ=st.median(w[0] for w in ws), K=st.median(w[1] for w in ws),
                    B=st.median(w[2] for w in ws), share=sum(1 for w in ws if w[1] < w[3]) / len(ws),
                    cap=st.median(w[3] for w in ws), good=st.mean(w[4] for w in ws)))
    print("ledger    cell     arm      rows   cap   occ(u/c)  K_med  quinn_backlog_B  share_K<c  good")
    for k in sorted(res, key=lambda k: (k[0], k[1], len(k[2]), k[2])):
        v = res[k]
        md = lambda f: st.median(x[f] for x in v)
        print(f"{k[0]:9s} {k[1]:8s} {k[2]:8s} {len(v):4d} {md('cap'):6.0f} {md('occ'):8.2f} {md('K'):6.0f} "
              f"{md('B'):15.0f} {st.mean(x['share'] for x in v):9.2f} {st.mean(x['good'] for x in v):6.1f}")


if __name__ == "__main__":
    main()
