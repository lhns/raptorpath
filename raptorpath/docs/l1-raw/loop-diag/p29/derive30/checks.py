"""(1) corr of Delta ln D and Delta ln T per [PCAP] period; (2) block variance of ln Pi over k consecutive
static-cap [LOOPDIAG] windows; (3) quinn cwnd vs the store cap in bytes during PWR rows."""
import glob, math, os, re, sys, statistics as st
from collections import defaultdict

KV = re.compile(r"(\w+)=([^\s]+)")
f_ = lambda s: float(s.rstrip("s"))
P29, P27 = sys.argv[1], sys.argv[2]
SYM = 1200.0

print("(1) Delta ln D vs Delta ln T per period (pooled per cell)")
acc = defaultdict(list)
for f in sorted(glob.glob(P29 + "/*-PWR-*-c.log")):
    cell = os.path.basename(f).split("-PWR-")[0]
    for l in open(f, errors="replace"):
        if l.startswith("[PCAP]"):
            p = dict(KV.findall(l))
            if int(p["period"]) < 3:
                continue
            dp, dm, pp, pm = f_(p["d_p"]), f_(p["d_m"]), f_(p["pi_p"]), f_(p["pi_m"])
            acc[cell].append((math.log(dp / dm), math.log((dp * dp / pp) / (dm * dm / pm))))
for c, v in acc.items():
    x = [a for a, _ in v]; y = [b for _, b in v]
    print(f"  {c}: sd(dlnD)={st.pstdev(x):.3f} sd(dlnT)={st.pstdev(y):.3f} corr={st.correlation(x, y):+.3f} "
          f"sd(dlnD+dlnT)={st.pstdev([a + b for a, b in v]):.3f}")


def wins(f):
    by_t = defaultdict(list)
    for l in open(f, errors="replace"):
        if l.startswith("[LOOPDIAG]"):
            kv = dict(KV.findall(l))
            by_t[f_(kv["t"])].append(kv)
    out = []
    for t in sorted(by_t):
        d = sum(float(p["dlv_rate"]) for p in by_t[t])
        ts = [float(p["rtt_p50_us"]) for p in by_t[t] if p["rtt_p50_us"] != "-"]
        if t >= 1.0 and d > 0 and ts:
            out.append(2 * math.log(d) - math.log(max(ts) / 1e6))
    return out


print("(2) block variance: var(adjacent block-mean differences)/2 over k windows (250 ms each)")
for cell, cap in (("c2-100", 192), ("c1s-400", 384), ("c1d-400", 384), ("c8-100", 384)):
    rows = [wins(f) for f in glob.glob(f"{P27}/{cell}-DEF{cap}-*-c.log")]
    line = []
    for k in (1, 2, 4):
        ds = []
        for r in rows:
            r = r[:-2]  # drop the transfer tail
            bm = [st.mean(r[i:i + k]) for i in range(0, len(r) - k + 1, k)]
            ds += [b - a for a, b in zip(bm, bm[1:])]
        if len(ds) >= 3:
            line.append(f"k={k}: {st.pvariance(ds) / 2:.5f} (n={len(ds)})")
    print(f"  {cell} DEF{cap}: " + "  ".join(line))

print("(3) quinn cwnd (bytes) vs store cap n-bar*(1+q/2)*SYM during PWR rows (post 1 s windows)")
for f in sorted(glob.glob(P29 + "/*-PWR-*-c.log")):
    name = os.path.basename(f)[:-6]
    caps, by_t = [], defaultdict(float)
    for l in open(f, errors="replace"):
        if l.startswith("[PCAP]"):
            p = dict(KV.findall(l))
            caps.append((f_(p["t"]), f_(p["nbar"]) * 1.05))
        elif l.startswith("[LOOPDIAG]"):
            p = dict(KV.findall(l))
            if f_(p["t"]) >= 1.0 and p["q_cwnd"] != "-" and p["dlv"] != "0":
                by_t[f_(p["t"])] += float(p["q_cwnd"])  # sum over paths that delivered
    cw = sorted(by_t.items())
    below, n = 0, 0
    ratios = []
    for t, c in cw:
        prior = [cp for tt, cp in caps if tt <= t]
        if not prior:
            continue
        capb = prior[-1] * SYM
        n += 1
        below += c < capb
        ratios.append(c / capb)
    if n:
        print(f"  {name}: windows={n} cwnd<cap in {below} ({below / n:.0%}); cwnd/cap p10={sorted(ratios)[n // 10]:.1f} "
              f"p50={st.median(ratios):.1f}")
