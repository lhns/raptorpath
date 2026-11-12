"""Status §34 (b4): the receiver's own capacity, read from the server [RDIAG] line.

Per [RDIAG] line with busy >= BUSY_MIN %: mu_R = msgs/s / (busy/100), the message
rate the receiver task could sustain at its measured cost per message if it were
never idle (msgs = data datagrams handled by the receiver task). Reported per
(cell, arm): the median mu_R, its cv over in-transfer lines, the median busy, and
mu_R in symbols/s against the cell's link datagram rate L (c1 legs 97 700/s, c2
9 770/s; c1d and c8 sum their legs) and against the delivered rate.
Usage: python -I rcap.py <glob> ...   (server logs, *-s.log)"""
import glob, os, re, sys, statistics as st
from collections import defaultdict

BUSY_MIN = float(os.environ.get("BUSY_MIN", "20"))
RD = re.compile(r"\[RDIAG\] busy=(\d+)% msgs=(\d+)/s")
L = {"c1s-400": 97700, "c1d-400": 2 * 97700, "c2-100": 9770, "c8-100": 9770 + 1954}


def main():
    res = defaultdict(list)
    for pat in sys.argv[1:]:
        for f in sorted(glob.glob(pat)):
            m = re.match(r"(c\w+-\d+)-([A-Z]+\d*)-s(\d+)-r(\d+)", os.path.basename(f))
            if not m:
                continue
            pts = []
            for line in open(f, errors="replace"):
                r = RD.search(line)
                if r:
                    b, n = float(r.group(1)), float(r.group(2))
                    if b >= BUSY_MIN and n > 0:
                        pts.append((b, n, n / (b / 100)))
            if len(pts) >= 3:
                res[(m.group(1), m.group(2))].append(pts)
    print(f"BUSY_MIN={BUSY_MIN}")
    print("cell     arm   rows lines busy_med msgs_med  mu_R_med  cv(mu_R)  mu_R/L  msgs_med/L")
    for k in sorted(res):
        allp = [p for row in res[k] for p in row]
        mus = [p[2] for p in allp]
        cv = st.mean(st.pstdev([p[2] for p in row]) / st.mean([p[2] for p in row]) for row in res[k])
        print(f"{k[0]:8s} {k[1]:5s} {len(res[k]):4d} {len(allp):5d} {st.median(p[0] for p in allp):8.0f} "
              f"{st.median(p[1] for p in allp):8.0f} {st.median(mus):9.0f} {cv:8.2f} "
              f"{st.median(mus) / L[k[0]]:7.2f} {st.median(p[1] for p in allp) / L[k[0]]:9.2f}")


if __name__ == "__main__":
    main()
