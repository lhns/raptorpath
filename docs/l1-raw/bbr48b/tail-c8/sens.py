# POST HOC sensitivity of the per-flight-path price: only a fraction f of the K/C uplift realised (f=1 is price.py's pf column).
import re, sys
txt = open(sys.argv[1]).read()
for blk in txt.split("== ")[1:]:
    arm = blk.split()[0]; mut = 498.0 if arm == "BASE" else 501.0
    rows = []
    for l in blk.splitlines():
        if not re.match(r"\s+\d+-\s?\d+s", l): continue
        a, b, c = l.split("|")[:3]
        rows.append([float(x) for x in a.split("s", 1)[1].split()] + [float(b.split()[0]), float(b.split()[1]), float(c)])
    out = []
    for f in (0.25, 0.5, 0.75, 1.0):
        H = 0.0
        for C, c0, c1, s1, K, kc, kpf, lam in rows:
            mu = kc * (1 + f * (kpf / kc - 1)); H = max(0.0, H + (lam - mu) * 4)
        out.append(f"f={f}: H0={H:.0f} tail={max(0.15, H/mut):.2f}s")
    print(arm, " | ".join(out))
