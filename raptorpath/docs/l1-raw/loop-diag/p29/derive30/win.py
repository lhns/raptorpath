"""Variance of ln Pi over 250 ms [LOOPDIAG] windows at a STATIC cap (no controller):
tests whether one-round noise averages as 1/W out to W ~ 0.25 s / T."""
import glob, math, os, re, sys, statistics as st
from collections import defaultdict

KV = re.compile(r"(\w+)=([^\s]+)")
ALPHA = float(os.environ.get("ALPHA", "2"))


def windows(f):
    by_t = defaultdict(list)
    for line in open(f, errors="replace"):
        if line.startswith("[LOOPDIAG]"):
            kv = dict(KV.findall(line))
            by_t[float(kv["t"].rstrip("s"))].append(kv)
    out = []
    for t in sorted(by_t):
        ps = by_t[t]
        d = sum(float(p["dlv_rate"]) for p in ps)
        ts = [float(p["rtt_p50_us"]) / 1e6 for p in ps if p["rtt_p50_us"] != "-"]
        if t >= 1.0 and d > 0 and ts:
            out.append((t, d, max(ts)))
    if not out:
        return []
    dm = st.median(x[1] for x in out)
    return [x for x in out if x[1] >= 0.5 * dm]


res = defaultdict(list)
for pat in sys.argv[1:]:
    for f in sorted(glob.glob(pat)):
        name = os.path.basename(f)[:-6]
        m = re.match(r"(c\w+-\d+)-(KNEE|DEF\d+)-", name)
        if not m:
            continue
        ws = windows(f)
        if len(ws) < 6:
            continue
        lp = [ALPHA * math.log(d) - math.log(t) for _, d, t in ws]
        lD = [math.log(d) for _, d, _ in ws]
        lT = [math.log(t) for _, _, t in ws]
        dl = [b - a for a, b in zip(lp, lp[1:])]
        s2 = st.pvariance(dl) / 2
        tmed = st.median(t for _, _, t in ws)
        span = (ws[-1][0] - ws[0][0]) / (len(ws) - 1)  # window spacing (s)
        res[(m.group(1), m.group(2))].append((s2, tmed, span, len(ws),
            st.pvariance([b - a for a, b in zip(lD, lD[1:])]) / 2,
            st.pvariance([b - a for a, b in zip(lT, lT[1:])]) / 2))

for k in sorted(res):
    v = res[k]
    s2 = st.mean(x[0] for x in v)
    tm = st.median(x[1] for x in v)
    sp = st.median(x[2] for x in v)
    print(f"{k[0]:8s} {k[1]:8s} rows={len(v)} wins={sum(x[3] for x in v)} sigma2_win={s2:.5f} "
          f"T_med={tm*1e3:.2f}ms spacing={sp:.3f}s W_win={sp/tm:.1f} sigma2*W={s2*sp/tm:.4f} "
          f"vD={st.mean(x[4] for x in v):.5f} vT={st.mean(x[5] for x in v):.5f}")
