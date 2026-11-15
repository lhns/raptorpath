# POST HOC, not registered. Q2b: where in the run do the c8 CAND UNDER lines sit? (episode t-ranges vs run length)
import os, re, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
D = sys.argv[1]; L = os.path.join(D, "full/diag-ld")
LINK = [12.5e6, 2.5e6]
for f in sorted(os.listdir(L)):
    if not (f.startswith("c8-100@def-CAND-") and f.endswith("-c.log")): continue
    qcc = []; diag = []
    for ln in open(os.path.join(L, f), errors="replace"):
        m = re.match(r"\[QCC\] t=([0-9.]+)s", ln)
        if m: qcc.append((float(m.group(1)), {int(a): (b, int(c)) for a, b, c in re.findall(r"p(\d+):qm=(\S) qrt_us=\d+ qsr_us=\d+ qbw=(\d+)", ln)}))
        m = re.match(r"\[DIAG\] t=([0-9.]+)s .*?good=([0-9.]+)Mbit", ln)
        if m: diag.append((float(m.group(1)), float(m.group(2)), re.search(r"\bsrc=(\d+)sym/s", ln).group(1), re.search(r"paused=(\d+)%", ln).group(1)))
    tmax = max(t for t, *_ in diag)
    eps = []
    for t, q in qcc:
        if t < 1: continue
        u = [i for i in (0, 1) if i in q and q[i][1] < 0.8 * LINK[i] and q[i][0] != "R"]
        if u: eps.append((t, u, {i: round(q[i][1] / LINK[i], 2) for i in q}, [q[i][0] for i in q]))
    if eps:
        ts = [e[0] for e in eps]
        d = {t: (g, s, p) for t, g, s, p in diag}
        print(f"{f[:24]} run_end_t={tmax:.1f} UNDER lines={len(eps)} t from {min(ts):.1f} to {max(ts):.1f}; first: {eps[0]}; last: {eps[-1]}; diag at last: {d.get(eps[-1][0])}")
    else:
        print(f"{f[:24]} run_end_t={tmax:.1f} no UNDER lines")
