"""Status §42 Next-1 optional: does S follow cap moves in p29 PWR rows? The power law moves the cap
every ~75 ms period (PCAP lines), ~3x per 250 ms [LOOPDIAG] window, so single steps are not resolvable;
this reads the window-mean nbar (PCAP periods ending in (t-0.25, t]) against the window's S (p0, and
the sum over legs of L_i*S_i for 2-leg cells), lag-correlating d ln nbar_w with dS_{w+k}, k = 0..4.
Windows t >= 1.0 s with n > 0 on every leg. Usage: python -I s42step.py <p29 diag-ld dir>"""
import glob, math, os, re, sys, statistics as st
from collections import defaultdict
LD = re.compile(r"\[LOOPDIAG\] t=([\d.]+)s p(\d+) n=(\d+) ov=\d+ rtt_min_us=(\S+) rtt_p10_us=\S+ rtt_p50_us=(\S+)")
PC = re.compile(r"\[PCAP\] t=([\d.]+)s period=\d+ nbar=([\d.]+)")
LINK = {"c2-100": [12.5e6], "c1s-400": [125e6], "c8-100": [12.5e6, 2.5e6], "c1d-400": [125e6, 125e6]}


def corr(x, y):
    if len(x) < 4:
        return float("nan")
    mx, my = st.mean(x), st.mean(y)
    sx = math.sqrt(sum((a - mx) ** 2 for a in x)); sy = math.sqrt(sum((b - my) ** 2 for b in y))
    return sum((a - mx) * (b - my) for a, b in zip(x, y)) / (sx * sy) if sx and sy else float("nan")


res = defaultdict(lambda: defaultdict(lambda: ([], [])))
steps = defaultdict(list)
for f in sorted(glob.glob(os.path.join(sys.argv[1], "*-PWR-*-c.log"))):
    cell = re.match(r"(c\w+-\d+)-PWR", os.path.basename(f)).group(1)
    nleg = len(LINK[cell])
    by_t, pcs = defaultdict(dict), []
    for l in open(f, errors="replace"):
        m = LD.search(l)
        if m:
            by_t[float(m.group(1))][int(m.group(2))] = m
        m = PC.search(l)
        if m:
            pcs.append((float(m.group(1)), float(m.group(2))))
    ser = []
    for t in sorted(by_t):
        if t < 1.0:
            continue
        legs = [by_t[t].get(i) for i in range(nleg)]
        if any(m is None or int(m.group(3)) == 0 or "-" in (m.group(4), m.group(5)) for m in legs):
            ser.append(None); continue
        S = sum(LINK[cell][i] / 1279.0 * (float(m.group(5)) - float(m.group(4))) / 1e6 for i, m in enumerate(legs))
        nb = [v for tt, v in pcs if t - 0.25 < tt <= t + 1e-9]
        ser.append((t, st.mean(nb), S, len(nb)) if nb else None)
    for w in ser:
        if w:
            steps[cell].append(w[3])
    for k in range(5):
        for i in range(1, len(ser) - k):
            a, b, c, d = ser[i - 1], ser[i], ser[i - 1 + k], ser[i + k]
            if a and b and c and d:
                res[cell][k][0].append(math.log(b[1] / a[1]))
                res[cell][k][1].append(d[2] - c[2])
for cell in res:
    print(f"{cell}: PCAP periods per window median {st.median(steps[cell])}; "
          + "  ".join(f"lag{k}: r={corr(*res[cell][k]):+.2f} (n={len(res[cell][k][0])})" for k in range(5)))
