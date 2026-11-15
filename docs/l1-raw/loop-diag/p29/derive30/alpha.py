"""sigma1^2(alpha) = var(Delta_alpha)/2 from [PCAP] (T+- recovered as d^2/pi), pooled per cell."""
import glob, math, os, re, sys, statistics as st
KV = re.compile(r"(\w+)=([^\s]+)")
f_ = lambda s: float(s.rstrip("s"))
out = {}
for f in sorted(glob.glob(sys.argv[1] + "/*-PWR-*-c.log")):
    cell = os.path.basename(f).split("-PWR-")[0]
    ps = [dict(KV.findall(l)) for l in open(f, errors="replace") if l.startswith("[PCAP]")][3:]
    for a in (0.5, 1.0, 2.0):
        ds = []
        for p in ps:
            dp, dm, pp, pm = f_(p["d_p"]), f_(p["d_m"]), f_(p["pi_p"]), f_(p["pi_m"])
            Tp, Tm = dp * dp / pp, dm * dm / pm
            ds.append(a * (math.log(dp) - math.log(dm)) - (math.log(Tp) - math.log(Tm)))
        out.setdefault((cell, a), []).append((st.pvariance(ds) / 2, len(ds)))
for k in sorted(out):
    v = out[k]
    print(k, "sigma1^2 pooled", round(sum(s * n for s, n in v) / sum(n for _, n in v), 4))
