"""Status §34 (a): window noise of ln Pi from the [DIAG] line, the only per-window
record the §32 ledgers carry (no [LOOPDIAG] on main).

Per 250 ms [DIAG] window (t >= 1 s): D = good (cumulative-ack advance / window, the
§6.1 D), T = rtt (max over paths of the estimator's smoothed RTT at the window's end),
cap = the win= denominator. ln Pi = ALPHA*ln D - ln T. sigma2_win = var(successive
difference of ln Pi)/2 per row, averaged over rows (the §30 win.py statistic).
Windows with D < 0.5 * the row's median D are dropped, as in win.py.

The same statistic on §27 DEF / §29 rows (which also carry [DIAG] beside
[LOOPDIAG]) calibrates it against §30's [LOOPDIAG] statistic (window rtt_p50
in place of the smoothed RTT), on the same rows.
Usage: python -I diagwin.py <glob> ...   (client logs, *-c.log); ROWS=1 prints rows."""
import glob, math, os, re, sys, statistics as st
from collections import defaultdict

ALPHA = float(os.environ.get("ALPHA", "2"))
KV = re.compile(r"(\w+)=([^\s\]]+)")
DIAG = re.compile(r"^\[DIAG\] t=([\d.]+)s win=(\d+)/(\d+) .*? good=([\d.]+)Mbit .*? rtt=([\d.]+)ms ")
QC = re.compile(r"qcwnd=(\d+)")


def diag_windows(f):
    out = []
    for line in open(f, errors="replace"):
        m = DIAG.match(line)
        if not m:
            continue
        t, used, cap, g, r = (float(m.group(1)), int(m.group(2)), int(m.group(3)),
                              float(m.group(4)), float(m.group(5)))
        q = sum(int(x) for x in QC.findall(line))
        if t >= 1.0 and g > 0 and r > 0:
            out.append((t, g, r / 1e3, cap, used, q))
    if not out:
        return []
    dm = st.median(x[1] for x in out)
    return [x for x in out if x[1] >= 0.5 * dm]


def loop_windows(f):
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


def s2(xs):
    d = [b - a for a, b in zip(xs, xs[1:])]
    return st.pvariance(d) / 2 if len(d) >= 2 else float("nan")


def main():
    res = defaultdict(list)
    for pat in sys.argv[1:]:
        for f in sorted(glob.glob(pat)):
            name = os.path.basename(f)[:-6]
            m = re.match(r"(c\w+-\d+)-([A-Z]+\d*)-s(\d+)-r(\d+)", name)
            if not m:
                continue
            ledger = os.path.basename(os.path.dirname(os.path.dirname(f)))
            key = (ledger, m.group(1), m.group(2))
            ws = diag_windows(f)
            if len(ws) < 6:
                continue
            lp = [ALPHA * math.log(g) - math.log(r) for _, g, r, *_ in ws]
            lD = [math.log(g) for _, g, *_ in ws]
            lT = [math.log(r) for _, _, r, *_ in ws]
            lw = loop_windows(f)
            lps = s2([ALPHA * math.log(d) - math.log(t) for _, d, t in lw]) if len(lw) >= 6 else float("nan")
            res[key].append(dict(s2=s2(lp), vD=s2(lD), vT=s2(lT), n=len(ws),
                                 cap=st.median(x[3] for x in ws), used=st.median(x[4] for x in ws),
                                 good=st.mean(x[1] for x in ws), T=st.median(x[2] for x in ws),
                                 loop_s2=lps, qc=st.median(x[5] for x in ws)))

    print(f"ALPHA={ALPHA}")
    print("ledger    cell     arm      rows wins cap_med used_med good_mean T_med_ms  s2_diag   vD_diag  vT_diag   s2_loop  diag/loop")
    for k in sorted(res):
        v = res[k]
        mean = lambda fld: st.mean(x[fld] for x in v)
        lo = [x["loop_s2"] for x in v if not math.isnan(x["loop_s2"])]
        lom = st.mean(lo) if lo else float("nan")
        ratio = mean("s2") / lom if lo else float("nan")
        print(f"{k[0]:9s} {k[1]:8s} {k[2]:8s} {len(v):4d} {sum(x['n'] for x in v):4d} "
              f"{st.median(x['cap'] for x in v):7.0f} {st.median(x['used'] for x in v):8.0f} "
              f"{mean('good'):9.1f} {st.median(x['T'] for x in v) * 1e3:8.2f} "
              f"{mean('s2'):8.5f} {mean('vD'):8.5f} {mean('vT'):8.5f} {lom:9.5f} {ratio:7.2f}")
        if os.environ.get("ROWS"):
            for x in v:
                print(f"      row s2={x['s2']:.5f} vD={x['vD']:.5f} vT={x['vT']:.5f} n={x['n']} "
                      f"good={x['good']:.1f} cap={x['cap']:.0f}")


if __name__ == "__main__":
    main()
