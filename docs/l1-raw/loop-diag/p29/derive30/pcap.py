"""Per-cell noise of ln Pi from the p29 [PCAP] ledgers (one measured round per half)."""
import glob, math, os, re, sys, statistics as st

D = sys.argv[1]  # .../p29/diag-ld
Q = 0.1
L = math.log((1 + Q / 2) / (1 - Q / 2))
KV = re.compile(r"(\w+)=([^\s]+)")


def rows():
    for f in sorted(glob.glob(os.path.join(D, "*-PWR-*-c.log"))):
        name = os.path.basename(f)[:-6]
        ps = []
        for line in open(f, errors="replace"):
            if line.startswith("[PCAP]"):
                kv = dict(KV.findall(line))
                ps.append({k: kv[k] for k in kv})
        yield name, ps


def fnum(s):
    return float(s.rstrip("s"))


def mad_sd(xs):
    m = st.median(xs)
    return 1.4826 * st.median([abs(x - m) for x in xs])


def ols_resid(x, y, deg=2):
    # least squares poly fit via normal equations (small deg)
    n = deg + 1
    A = [[sum(xi ** (i + j) for xi in x) for j in range(n)] for i in range(n)]
    b = [sum((xi ** i) * yi for xi, yi in zip(x, y)) for i in range(n)]
    # gaussian elimination
    for c in range(n):
        p = max(range(c, n), key=lambda r: abs(A[r][c]))
        A[c], A[p] = A[p], A[c]
        b[c], b[p] = b[p], b[c]
        for r in range(n):
            if r != c and A[c][c] != 0:
                f = A[r][c] / A[c][c]
                A[r] = [a - f * ac for a, ac in zip(A[r], A[c])]
                b[r] -= f * b[c]
    coef = [b[i] / A[i][i] for i in range(n)]
    return [yi - sum(coef[k] * xi ** k for k in range(n)) for xi, yi in zip(x, y)]


def acf(r, lag):
    m = st.mean(r)
    v = sum((x - m) ** 2 for x in r)
    return sum((r[i] - m) * (r[i + lag] - m) for i in range(len(r) - lag)) / v


cells = {}
for name, ps in rows():
    cell = name.split("-PWR-")[0]
    if not ps:
        continue
    # skip the first 3 periods (seed transient)
    use = ps[3:]
    dl = [math.log(fnum(p["pi_p"])) - math.log(fnum(p["pi_m"])) for p in use]
    # half series in time order: (+ then -) per period
    lc, lp, lD, lT = [], [], [], []
    for p in use:
        nb = fnum(p["nbar"])
        t = fnum(p["t_loop_us"]) / 1e6  # only the - half's T is logged
        for s, key, dk in ((+1, "pi_p", "d_p"), (-1, "pi_m", "d_m")):
            lc.append(math.log(nb * (1 + s * Q / 2)))
            lp.append(math.log(fnum(p[key])))
            lD.append(math.log(max(fnum(p[dk]), 1.0)))
    res = ols_resid(lc, lp, 2)
    resD = ols_resid(lc, [2 * x for x in lD], 2)
    # per-period D-only and T-only decomposition of Delta: ln Pi = 2 ln D - ln T
    dD = [2 * (math.log(max(fnum(p["d_p"]), 1)) - math.log(max(fnum(p["d_m"]), 1))) for p in use]
    dT = [a - b for a, b in zip(dl, dD)]  # = -(ln T+ - ln T-)
    rounds = [fnum(p["t_loop_us"]) / 1e6 for p in use]
    t_end = fnum(ps[-1]["t"])
    per = (t_end - fnum(ps[0]["t"])) / max(len(ps) - 1, 1)
    r = dict(
        n=len(use), var_dl=st.pvariance(dl), sd_dl=st.pstdev(dl), mad_dl=mad_sd(dl),
        sd_res=st.pstdev(res), mad_res=mad_sd(res), acf1=acf(res, 1), acf2=acf(res, 2),
        acf4=acf(res, 4), sd_dD=st.pstdev(dD), sd_dT=st.pstdev(dT), mad_dD=mad_sd(dD), mad_dT=mad_sd(dT),
        clamp=sum(abs(x / L) > 1 for x in dl) / len(dl), T_med=st.median(rounds), per=per,
        n0=fnum(ps[0]["nbar"]), nend=fnum(ps[-1]["next"]), t_end=t_end,
        nbar_mean=st.mean(fnum(p["nbar"]) for p in ps), d_med=st.median(fnum(p["d_m"]) for p in use),
        sd_resD=st.pstdev(resD),
    )
    cells.setdefault(cell, []).append((name, r))

for cell, rs in cells.items():
    print("==", cell)
    for name, r in rs:
        print(" ", name, " ".join(f"{k}={v:.4g}" for k, v in r.items()))
