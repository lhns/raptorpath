"""Proof stage 1: the deciding row (three-knobs-proof-plan.md section 3), repair off.  Builders, runner and reader in one file.

    python proof_s1.py list [--part AA|DECIDE|CROSS]
    python proof_s1.py run  --part AA|DECIDE|CROSS [--seeds 12] [--workers 1]      # one file per run, resumable
    python proof_s1.py freeze                                                       # A/A -> nf_proof_s1.json, once, before any row is read
    python proof_s1.py read [--cross]                                               # the table, the comparisons, the verdict (markdown)

Cell: CL-long@b (the engine-like sweep, plan A5), repair off.  The repair-on half of the plan's DECIDE part (arm RL) needs stage 3e's
rate law and decoder, which do not exist on this base: it is owed.  Every row is named by what its sender and receiver do (F6); the
code names stand in parentheses.  Output: out_proof_s1/<PART>/<name>-<seed>.json.
"""
import argparse
import json
import math
import multiprocessing as mp
import os
import sys
import time

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import toy2  # noqa: E402
import batch3  # noqa: E402
import batch3d  # noqa: E402

OUT = os.path.join(HERE, "out_proof_s1")
NF_PATH = os.path.join(HERE, "nf_proof_s1.json")
INF = math.inf
CELL = "CL-long@b"
SEEDS = 12
TAU = 0.4                                      # the call / realtime tunnel deadline (OQ5)
T_FINE = [round(1.2 + 0.05 * i, 2) for i in range(9)]                    # 1.2 .. 1.6 in 0.05 s steps
T_SET = [0.4, 0.6, 0.8, 1.0] + T_FINE + [2.0, INF]                       # 15 points, plan 1.4; always includes T = inf
H_INF_T = [0.4, 1.4, INF]                                                # H = inf at three of the T points
RHO_SET = [0.95, 0.98, 0.99, 0.995, 1.0]                                 # ADR-0084 at rho, plan 1.2
DELTA_SET = [0.5, 5.0, 50.0]                                             # today's Realtime, budget derived
tag = batch3.tag


# ---- the arms, named by what the sender and the receiver do (F6) -------------------------------------------------
def n_lt(T, H):
    return "lifetime T=%s, hold %s (S5)" % (tag(T), "inf" if H == INF else tag(H))


def n_adr(rho):
    return "ADR-0084 sender at rho=%s, today's receiver (C1)" % tag(rho)


def n_rt(d):
    return "today's Realtime, repair-off control, delta=%s (E-rt)" % tag(d)


N_BULK = "never gives up, strict order = today's Bulk (C3)"
N_X_ADR_ANY = "ADR-0084 sender at rho=0.99, any-order receiver (cross)"
N_X_ADR_NOCOPY = "ADR-0084 sender at rho=0.99 without its copy, today's receiver (cross, S1)"
N_AA = "A/A: lifetime T=inf, hold 0, phase variant 1 (S5)"


def lt_cfg(seed, T, H, variant=0):
    return batch3d.s5_cfg(CELL, seed, T, H, variant=variant)


def adr_cfg(seed, rho, **kw):
    lc, d = batch3d.base(CELL)
    k = dict(sender="S1c", recv="R0e")
    k.update(kw)
    return toy2.Cfg(cell=lc, seed=seed, delta=50.0, rho=rho, **k, **d)


def rt_cfg(seed, delta):
    """Today's Realtime sender and receiver (plan 2.3), repair off: ADR-0084's sender with its six differences taken out one by one."""
    lc, d = batch3d.base(CELL)
    d.update(pause_B=10 ** 9)                  # no source pause: the link queue drops instead
    return toy2.Cfg(cell=lc, sender="S1", recv="R0e", seed=seed, delta=delta, rho=1.0, check_p1=False,
                    d_arm="EWMA", budget="resid", floor_send=False, evict_window=200, r0e_max=500, **d)


def bulk_cfg(seed):
    return batch3d.cmp_cfg(CELL, "C3", seed)


JOBS = {}


def job(part, name, fn):
    assert name not in JOBS, name
    JOBS[name] = (part, fn)


def build():
    JOBS.clear()
    for T in T_SET:
        job("DECIDE", n_lt(T, 0.0), lambda s, T=T: lt_cfg(s, T, 0.0))
    for T in H_INF_T:
        job("DECIDE", n_lt(T, INF), lambda s, T=T: lt_cfg(s, T, INF))
    for rho in RHO_SET:
        job("DECIDE", n_adr(rho), lambda s, rho=rho: adr_cfg(s, rho))
    for d in DELTA_SET:
        job("DECIDE", n_rt(d), lambda s, d=d: rt_cfg(s, d))
    job("DECIDE", N_BULK, lambda s: bulk_cfg(s))
    job("AA", N_AA, lambda s: lt_cfg(s, INF, 0.0, variant=1))
    # the conditional 2 x 2 (plan 3.4): sender {ADR-0084 at 0.99, never gives up} x receiver {today's Realtime, any order}; two cells exist already
    job("CROSS", N_X_ADR_ANY, lambda s: adr_cfg(s, 0.99, recv="RH", Hk=0.0))
    job("CROSS", N_X_ADR_NOCOPY, lambda s: adr_cfg(s, 0.99, sender="S1"))


def fname(name, seed):
    safe = "".join(ch if ch.isalnum() or ch in "=.-" else "_" for ch in name)
    return os.path.join(OUT, JOBS[name][0], "%s-%d.json" % (safe, seed))


def work(task):
    n, seed = task
    t = time.time()
    r = batch3d.run_one(JOBS[n][1](seed))
    r["wall_s"] = time.time() - t
    batch3d.dump(fname(n, seed), r)
    return n, seed, r["wall_s"]


def weight(n):
    return -(3.0 if "C1" in n else 1.0)


def cmd_run(a):
    build()
    names = [n for n, (p, _) in JOBS.items() if p in a.part.split(",")]
    tasks = [(n, s) for n in names for s in range(a.seeds) if not os.path.exists(fname(n, s))]
    tasks.sort(key=lambda t: weight(t[0]))
    t0 = time.time()
    print("%d runs to do (%d configurations)" % (len(tasks), len(names)), flush=True)
    if a.workers <= 1:
        it = (work(t) for t in tasks)
        pool = None
    else:
        pool = mp.Pool(a.workers, initializer=build)
        it = pool.imap_unordered(work, tasks, chunksize=1)
    for i, (n, s, w) in enumerate(it, 1):
        print("[%6.0fs] %d/%d %s %d %.1fs" % (time.time() - t0, i, len(tasks), n, s, w), flush=True)
    print("[%6.0fs] done" % (time.time() - t0), flush=True)


# ---- the reader ----------------------------------------------------------------------------------------------------
READ = ("share_by_0p4", "recovery_time")                  # the two y readouts under the pass rule (plan 1.1)
BETTER = {"share_by_0p4": +1.0, "recovery_time": -1.0}    # share: higher is better; recovery: lower is better
REC_CAP = 14.5                                            # recovery "not within the run": censored at the readable end (18 s - 3.5 s)
GATE_KEYS = ("finished", "commit_viol", "l1_viol")


def load(part):
    d = {}
    for n in JOBS:
        if JOBS[n][0] != part:
            continue
        d[n] = {}
        for s in range(SEEDS):
            f = fname(n, s)
            if os.path.exists(f):
                d[n][s] = json.load(open(f))
    return d


def val(r, k):
    """One readout of one run.  recovery_time None ("not within the run") is censored at REC_CAP, counted, never dropped."""
    if k == "recovery_time":
        v = r.get("recovery_time")
        return REC_CAP if v is None else float(v)
    if k in r:
        return float(r[k])
    return float(r["gauges"][k])


def vec(rows, n, k):
    return np.asarray([val(rows[n][s], k) for s in sorted(rows[n])])


def boot(x, n=4000, seed=1):
    x = np.asarray(x, float)
    x = x[~np.isnan(x)]
    if len(x) == 0:
        return float("nan"), float("nan"), float("nan")
    if len(x) < 2 or np.all(x == x[0]):
        return float(x.mean()), float(x.mean()), float(x.mean())
    rng = np.random.default_rng(seed)
    m = x[rng.integers(0, len(x), (n, len(x)))].mean(1)
    return float(x.mean()), float(np.percentile(m, 2.5)), float(np.percentile(m, 97.5))


def read_good(g, m):
    """g = paired difference in the direction 'larger is better for the new model'.  The stage 3d reading rule against m."""
    mu, lo, hi = boot(g)
    if math.isnan(mu):
        return dict(mean=mu, lo=lo, hi=hi, m=m, word="n/a")
    if lo > m:
        w = "shown better"
    elif hi < -m:
        w = "shown worse"
    elif lo >= -m and hi <= m:
        w = "equivalent"
    else:
        w = "undetermined"
    return dict(mean=mu, lo=lo, hi=hi, m=m, word=w)


def t_bar(k, ref_mean, units):
    """Plan 1.2: shares max(1/units, 0.05 (1 - share_ref)); recovery max(5 % of the reference, 0.1 s)."""
    if k == "recovery_time":
        return max(0.05 * abs(ref_mean), 0.1)
    return max(1.0 / units, 0.05 * (1.0 - ref_mean))


def nf_of(nf, k):
    return float(nf.get(k, 0.0))


def merge_points(xs, ys, better, noise):
    """Plan 1.2: sort the design's points by x (seed means); where two neighbours have x within the noise of each other keep the one with the
    better y mean.  xs, ys: (points, seeds) arrays.  Returns the kept point indices in increasing x."""
    mx = xs.mean(1)
    order = list(np.argsort(mx))
    kept = []
    for i in order:
        if kept and abs(mx[i] - mx[kept[-1]]) <= noise:
            if better * ys[i].mean() > better * ys[kept[-1]].mean():
                kept[-1] = i
        else:
            kept.append(i)
    return kept


def curve_at(xs, ys, kept, xq):
    """Per seed, the straight-line interpolation of the curve (kept points, that seed's x and y) at that seed's xq; NaN outside its x range
    (no extrapolation, plan 1.2).  xs, ys: (points, seeds); xq: (seeds,)."""
    out = np.full(xq.shape, np.nan)
    for j in range(xq.shape[0]):
        px, py = xs[kept, j], ys[kept, j]
        o = np.argsort(px)
        px, py = px[o], py[o]
        if px[0] - 1e-12 <= xq[j] <= px[-1] + 1e-12:
            out[j] = np.interp(xq[j], px, py)
    return out


def compare(new_x, new_y, kept, base_x, base_y, k, nfk, nfx, units, ref_mean):
    """New curve against a baseline point (base_x, base_y: (seeds,)): the paired difference, signed so that larger = better for the new model."""
    yi = curve_at(new_x, new_y, kept, base_x)
    if np.any(np.isnan(yi)):
        return dict(word="outside the swept range", mean=float("nan"), lo=float("nan"), hi=float("nan"), m=float("nan"),
                    n_out=int(np.isnan(yi).sum()))
    g = BETTER[k] * (yi - base_y)
    m = max(t_bar(k, ref_mean, units), nfk)
    rd = read_good(g, m)
    rd["n_out"] = 0
    return rd


def compare_rev(base_x, base_y, kept, pt_x, pt_y, k, nfk, units, ref_mean):
    """A point of the new curve against the baseline curve (the other direction of 'curve against curve')."""
    yi = curve_at(base_x, base_y, kept, pt_x)
    if np.any(np.isnan(yi)):
        return dict(word="outside the swept range", mean=float("nan"), lo=float("nan"), hi=float("nan"), m=float("nan"),
                    n_out=int(np.isnan(yi).sum()))
    g = BETTER[k] * (pt_y - yi)
    m = max(t_bar(k, ref_mean, units), nfk)
    rd = read_good(g, m)
    rd["n_out"] = 0
    return rd


def fmt(rd, nd=3):
    if rd["word"].startswith("outside"):
        return "outside the swept range (%d seeds)" % rd["n_out"]
    return "%.*g [%.*g, %.*g] vs m=%.3g: %s" % (nd, rd["mean"], nd, rd["lo"], nd, rd["hi"], rd["m"], rd["word"])


def freeze(force=False):
    build()
    rows = load("DECIDE")
    aa = load("AA")
    a_name, aa_name = n_lt(INF, 0.0), N_AA
    if len(rows.get(a_name, {})) < SEEDS or len(aa.get(aa_name, {})) < SEEDS:
        raise SystemExit("the A/A needs the %d seeds of both arms" % SEEDS)
    nf = dict(notes="p95 over 12 seeds of |A - A'|, A = %s, A' = %s (phase variant 1); ms-free: shares and seconds" % (a_name, aa_name), cells={})
    for k in READ + ("share_any", "share_T"):
        d = np.abs(vec(rows, a_name, k) - np.asarray([val(aa[aa_name][s], k) for s in sorted(aa[aa_name])]))
        nf["cells"][k] = float(np.percentile(d, 95))
    if os.path.exists(NF_PATH) and not force:
        print("nf_proof_s1.json exists; not overwriting (frozen)")
        return
    json.dump(nf, open(NF_PATH, "w"), indent=1)
    print("frozen", NF_PATH, nf["cells"])


def cmd_read(a):
    build()
    rows = load("DECIDE")
    nf = json.load(open(NF_PATH))["cells"]
    L = []
    P = L.append
    names_new = [n_lt(T, 0.0) for T in T_SET]
    have = [n for n in names_new if len(rows.get(n, {})) == SEEDS]
    units = int(next(iter(rows[have[0]].values()))["units"])
    P("### The new model's curve and every row (repair off, CL-long@b, 12 paired seeds, means)\n")
    P("| row | share_any | on time at 0.4 s | on time at T | recovery (s) | intake expired (dequeue) | floor passes | gates: unfinished / commit / L1 | evict-window skips | sheds | budget refusals | queue drops | p1 viol |")
    P("|---|---|---|---|---|---|---|---|---|---|---|---|---|")
    allnames = [n for n in JOBS if JOBS[n][0] == "DECIDE"]
    for n in allnames:
        if len(rows.get(n, {})) < SEEDS:
            P("| %s | not run |" % n)
            continue
        f = lambda k: float(vec(rows, n, k).mean())
        rr = [rows[n][s].get("recovery_time") for s in sorted(rows[n])]
        cens = sum(1 for x in rr if x is None)
        unf = sum(1 for s in rows[n] if rows[n][s].get("finished") is False)
        P("| %s | %.4f | %.4f | %.4f | %.2f%s | %.0f | %.0f | %d / %.0f / %.0f | %.0f | %.0f | %.0f | %.0f | %.0f |" % (
            n, f("share_any"), f("share_by_0p4"), f("share_T"), f("recovery_time"), (" (%d censored)" % cens) if cens else "",
            f("intake_expired_deq"), f("pass_floor"), unf, f("commit_viol"), f("l1_viol"), f("evict_skip"), f("verdicts"),
            f("refusals"), f("queue_drop"), f("p1_viol")))
    # comparisons
    kept = {}
    X = {n: vec(rows, n, "share_any") for n in have}
    new_x = np.stack([vec(rows, n, "share_any") for n in have])
    comps = []

    def newcurve(k):
        y = np.stack([vec(rows, n, k) for n in have])
        kk = merge_points(new_x, y, BETTER[k], max(nf_of(nf, "share_any"), 1.0 / units))
        return y, kk
    P("\n### Matched-share comparisons (the new model's H = 0 curve, T sweep, against each baseline)\n")
    P("Reading rule: paired difference of the new curve (per seed, interpolated at that seed's baseline share) minus the baseline, signed so that positive favours the new model, against m = max(T_bar, A/A floor). No extrapolation.\n")
    P("| baseline | readout | new curve at the baseline's share minus the baseline | verdict |")
    P("|---|---|---|---|")
    base_pts = [n_adr(r) for r in RHO_SET] + [n_rt(d) for d in DELTA_SET]
    for bn in base_pts:
        if len(rows.get(bn, {})) < SEEDS:
            continue
        bx = vec(rows, bn, "share_any")
        for k in READ:
            ny, kk = newcurve(k)
            by = vec(rows, bn, k)
            rd = compare(new_x, ny, kk, bx, by, k, nf_of(nf, k), nf_of(nf, "share_any"), units, float(by.mean()))
            comps.append(("point", bn, k, rd))
            P("| %s | %s | %s | %s |" % (bn, k, fmt(rd), rd["word"]))
    P("\n### Curve against curve: the new model against ADR-0084's rho sweep, both directions\n")
    adr = [n_adr(r) for r in RHO_SET]
    adr = [n for n in adr if len(rows.get(n, {})) == SEEDS]
    if adr:
        ax = np.stack([vec(rows, n, "share_any") for n in adr])
        for k in READ:
            ny, kk = newcurve(k)
            ay = np.stack([vec(rows, n, k) for n in adr])
            ak = merge_points(ax, ay, BETTER[k], max(nf_of(nf, "share_any"), 1.0 / units))
            lo, hi = new_x[kk].mean(1).min(), new_x[kk].mean(1).max()
            for i in ak:      # ADR-0084's points inside the new curve's x range: that is the table above; the points outside are reported here
                if not (lo - 1e-12 <= ax[i].mean() <= hi + 1e-12):
                    P("- %s on %s: x = %.4f lies outside the new curve's range [%.4f, %.4f]: counts as not passed" % (adr[i], k, ax[i].mean(), lo, hi))
                    comps.append(("range", adr[i], k, dict(word="outside the swept range")))
            alo, ahi = ax[ak].mean(1).min(), ax[ak].mean(1).max()
            for j in kk:      # the new model's points inside ADR-0084's x range, read against ADR-0084's interpolated curve
                if alo - 1e-12 <= new_x[j].mean() <= ahi + 1e-12:
                    by = ny[j]
                    rd = compare_rev(ax, ay, ak, new_x[j], by, k, nf_of(nf, k), units, float(ay.mean()))
                    comps.append(("curve", have[j], k, rd))
                    P("- new %s against ADR-0084's curve, %s: %s" % (have[j], k, fmt(rd)))
    # gates and identity
    P("\n### Gates and identity\n")
    bad = []
    for n in allnames:
        for s in rows.get(n, {}):
            r = rows[n][s]
            if r.get("finished") is False or r["gauges"]["commit_viol"] or r["gauges"]["l1_viol"]:
                bad.append((n, s))
    P("- runs failing a gate (unfinished, commitment violation, L1 violation): %d" % len(bad))
    for x in bad[:20]:
        P("  - %s seed %d" % x)
    import summarize3c as S3
    idn = n_lt(INF, INF)
    if len(rows.get(idn, {})) == SEEDS and len(rows.get(N_BULK, {})) == SEEDS:
        nd = 0
        for s in range(SEEDS):
            for k in S3.ID_KEYS:
                a, b = rows[idn][s].get(k), rows[N_BULK][s].get(k)
                if a != b and not (isinstance(a, float) and isinstance(b, float) and math.isnan(a) and math.isnan(b)):
                    nd += 1
        P("- identity, lifetime T=inf with hold inf against today's Bulk (C3), %d keys x %d seeds: %d differences" % (len(S3.ID_KEYS), SEEDS, nd))
        bad.append(("identity", nd)) if nd else None
    worse = [c for c in comps if c[3]["word"] in ("shown worse", "outside the swept range")]
    better = [c for c in comps if c[3]["word"] == "shown better"]
    P("\n### Verdict against the plan's pass rule (this row)\n")
    P("- comparisons read: %d; shown worse or outside the swept range: %d; shown better: %d" % (len(comps), len(worse), len(better)))
    for c in worse:
        P("  - WORSE/OUTSIDE: %s | %s | %s" % (c[1], c[2], c[3]["word"]))
    ok = not worse and not bad
    P("- rule 1 (nowhere below): %s; rule 3 (gates) and rule 4 (identity): %s; rule 2 (above somewhere): %s" % (
        "holds" if not worse else "FAILS", "hold" if not bad else "FAIL", "yes" if better else "no, in this row"))
    P("- **ROW %s**" % ("PASSES" if ok else "FAILS"))
    print("\n".join(L))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("cmd", choices=("list", "run", "freeze", "read"))
    ap.add_argument("--part", default="DECIDE")
    ap.add_argument("--seeds", type=int, default=SEEDS)
    ap.add_argument("--workers", type=int, default=1)
    ap.add_argument("--force", action="store_true")
    a = ap.parse_args()
    if a.cmd == "run":
        cmd_run(a)
    elif a.cmd == "list":
        build()
        for n, (p, _) in JOBS.items():
            if p in a.part.split(","):
                print(p, n)
    elif a.cmd == "freeze":
        freeze(a.force)
    else:
        cmd_read(a)


if __name__ == "__main__":
    mp.freeze_support()
    main()
