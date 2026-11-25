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
N_AA04 = "A/A: lifetime T=0.4, hold 0, phase variant 1 (S5)"


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
    job("AA", N_AA04, lambda s: lt_cfg(s, 0.4, 0.0, variant=1))
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
    if a.arms_file:                                    # the registered doubling: only the arms of the undetermined comparisons
        keep = set(json.load(open(a.arms_file)))
        names = [n for n in names if n in keep]
    tasks = [(n, s) for n in names for s in range(a.seed0, a.seed0 + a.seeds) if not os.path.exists(fname(n, s))]
    tasks.sort(key=lambda t: weight(t[0]))
    t0 = time.time()
    print("%d runs to do (%d configurations)" % (len(tasks), len(names)), flush=True)
    if a.workers <= 1:
        it = (work(t) for t in tasks)
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


def load(part, nseeds=2 * SEEDS):
    d = {}
    for n in JOBS:
        if JOBS[n][0] != part:
            continue
        d[n] = {}
        for s in range(nseeds):
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


def curve_at(xs, ys, kept, xq, clamp_low=False):
    """Per seed, the straight-line interpolation of the curve (kept points, that seed's x and y) at that seed's xq; NaN outside its x range
    (no extrapolation, plan 1.2).  clamp_low (amendment 1.6c): a query below the curve's lowest delivered share reads the lowest point.
    xs, ys: (points, seeds); xq: (seeds,)."""
    out = np.full(xq.shape, np.nan)
    for j in range(xq.shape[0]):
        px, py = xs[kept, j], ys[kept, j]
        o = np.argsort(px)
        px, py = px[o], py[o]
        if px[0] - 1e-12 <= xq[j] <= px[-1] + 1e-12:
            out[j] = np.interp(xq[j], px, py)
        elif clamp_low and xq[j] < px[0]:
            out[j] = py[0]
    return out


def compare(new_x, new_y, kept, base_x, base_y, k, nfk, nfx, units, ref_mean, clamp_low=True):
    """New curve against a baseline point (base_x, base_y: (seeds,)): the paired difference, signed so that larger = better for the new model."""
    yi = curve_at(new_x, new_y, kept, base_x, clamp_low)
    if np.any(np.isnan(yi)):
        return dict(word="outside the swept range", mean=float("nan"), lo=float("nan"), hi=float("nan"), m=float("nan"),
                    n_out=int(np.isnan(yi).sum()), n_clamped=0)
    g = BETTER[k] * (yi - base_y)
    m = max(t_bar(k, ref_mean, units), nfk)
    rd = read_good(g, m)
    rd["n_out"] = 0
    lows = [new_x[kept, j].min() for j in range(base_x.shape[0])]
    rd["n_clamped"] = int(sum(1 for j in range(base_x.shape[0]) if base_x[j] < lows[j] - 1e-12))
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
    rd["n_clamped"] = 0
    return rd


def fmt(rd, nd=3):
    if rd["word"].startswith("outside"):
        return "outside the swept range (%d seeds)" % rd["n_out"]
    return "%.*g [%.*g, %.*g] vs m=%.3g: %s%s%s" % (nd, rd["mean"], nd, rd["lo"], nd, rd["hi"], rd["m"], rd["word"],
                                                    (" (against the curve's lowest point, %d seeds)" % rd["n_clamped"]) if rd.get("n_clamped") else "",
                                                    " [%s]" % rd["note"] if rd.get("note") else "")


RESOLUTION = {"share_any": None, "share_T": None, "share_by_0p4": None, "recovery_time": 0.1}      # shares: 1/units; recovery: the window step


def aa_pairs():
    """The two registered A/A pairs: variant 0 (a row of the sweep) against variant 1, at T = inf (hold 0) and at the primary lifetime (T = 0.4, hold 0)."""
    return [(n_lt(INF, 0.0), N_AA), (n_lt(0.4, 0.0), N_AA04)]


def freeze(force=False):
    """NF per readout = the larger of the two pairs' p95 over 12 seeds of |A - A'|.  A floor that is 0 or below the readout's resolution is
    degenerate: it is listed, and the reading rule's own bar (T_bar >= 1/units for shares, 0.1 s for recovery) then carries the comparison."""
    build()
    rows = load("DECIDE", SEEDS)
    aa = load("AA", SEEDS)
    units = int(rows[n_lt(INF, 0.0)][0]["units"])
    nf = dict(notes="per readout the larger of the p95 over 12 seeds of |A - A'| of the two pairs: %s" % "; ".join("A = %s, A' = %s" % x for x in aa_pairs()),
              cells={}, pairs={}, degenerate=[])
    for k in READ + ("share_any", "share_T"):
        vals = []
        for a_name, aa_name in aa_pairs():
            if len(rows.get(a_name, {})) < SEEDS or len(aa.get(aa_name, {})) < SEEDS:
                raise SystemExit("the A/A needs the %d seeds of both arms of each pair" % SEEDS)
            d = np.abs(np.asarray([val(rows[a_name][s], k) for s in range(SEEDS)]) - np.asarray([val(aa[aa_name][s], k) for s in range(SEEDS)]))
            vals.append(float(np.percentile(d, 95)))
        nf["pairs"][k] = vals
        nf["cells"][k] = max(vals)
        res = RESOLUTION[k] if RESOLUTION[k] is not None else 1.0 / units
        for (a_name, _), v in zip(aa_pairs(), vals):
            if v <= res:
                nf["degenerate"].append("%s at the pair of %s: %.3g (resolution %.3g)" % (k, a_name, v, res))
    if os.path.exists(NF_PATH) and not force:
        print("nf_proof_s1.json exists; not overwriting (frozen)")
        return
    json.dump(nf, open(NF_PATH, "w"), indent=1)
    print("frozen", NF_PATH, nf["cells"], "degenerate:", nf["degenerate"])


def classify(comps, gates_bad):
    """Amendment 1.6: the row's outcome.  Only comparisons with scope "verdict" count (ADR-0084's rho curve); today's Realtime rows are
    reported only.  FAILS: any shown worse or outside the swept range (above the curve), or a gate.  UNDETERMINED: none worse but some
    comparison undetermined: neither pass nor fail.  PASSES: every comparison equivalent or shown better."""
    v = [c for c in comps if c["scope"] == "verdict"]
    worse = [c for c in v if c["rd"]["word"] in ("shown worse", "outside the swept range")]
    undet = [c for c in v if c["rd"]["word"] in ("undetermined", "n/a")]
    better = [c for c in v if c["rd"]["word"] == "shown better"]
    if worse or gates_bad:
        out = "FAILS"
    elif undet:
        out = "UNDETERMINED"
    else:
        out = "PASSES"
    return out, worse, undet, better


def bracket(means, order, x):
    """Indices of the (up to two) kept points whose mean x brackets x; the lowest point when x lies below the curve."""
    o = sorted(order, key=lambda i: means[i])
    below = [i for i in o if means[i] <= x]
    above = [i for i in o if means[i] > x]
    return ([below[-1]] if below else []) + ([above[0]] if above else [])


def empty_curve_note(old_means, new_means, noise):
    """Why the curve-against-curve section is empty, in words: the old curve's range of delivered share and which new points fall inside it."""
    lo, hi = float(min(old_means)), float(max(old_means))
    inside = [x for x in new_means if lo - 1e-12 <= x <= hi + 1e-12]
    if inside:
        return ""
    why = ("the old curve's points all lie within the merge noise (%.3g) of each other, so it is one point on the share axis" % noise
           if hi - lo <= noise else "no point of the new curve lies in that range")
    return "No curve-against-curve comparison was possible: the old curve's delivered share spans [%.4f, %.4f] (width %.2g); %s. The matched-share comparison above is therefore a comparison at that one share." % (
        lo, hi, hi - lo, why)


def cmd_read(a):
    build()
    rows = load("DECIDE", a.nseeds)
    nf = json.load(open(NF_PATH))["cells"]
    L = []
    P = L.append

    def full(n, ns):
        return len(rows.get(n, {})) >= ns and all(s in rows[n] for s in range(ns))

    def arr(names, k, ns):
        return np.stack([[val(rows[n][s], k) for s in range(ns)] for n in names])

    names_new = [n_lt(T, 0.0) for T in T_SET]
    have = [n for n in names_new if full(n, SEEDS)]
    units = int(rows[have[0]][0]["units"])
    xnoise = max(nf_of(nf, "share_any"), 1.0 / units)
    P("### Every row (repair off, CL-long@b, paired seeds, means over the first %d seeds)\n" % SEEDS)
    P("| row | share_any | on time at 0.4 s | on time at T | recovery (s) | intake expired (dequeue) | floor passes | unfinished / commit / L1 | evict-window skips | sheds | budget refusals | queue drops | p1 viol |")
    P("|---|---|---|---|---|---|---|---|---|---|---|---|---|")
    allnames = [n for n in JOBS if JOBS[n][0] == "DECIDE"]
    for n in allnames:
        if not full(n, SEEDS):
            P("| %s | not run |" % n)
            continue
        f = lambda k: float(np.mean([val(rows[n][s], k) for s in range(SEEDS)]))
        rr = [rows[n][s].get("recovery_time") for s in range(SEEDS)]
        cens = sum(1 for x in rr if x is None)
        unf = sum(1 for s in range(SEEDS) if rows[n][s].get("finished") is False)
        P("| %s | %.4f | %.4f | %.4f | %.2f%s | %.0f | %.0f | %d / %.0f / %.0f | %.0f | %.0f | %.0f | %.0f | %.0f |" % (
            n, f("share_any"), f("share_by_0p4"), f("share_T"), f("recovery_time"), (" (%d censored)" % cens) if cens else "",
            f("intake_expired_deq"), f("pass_floor"), unf, f("commit_viol"), f("l1_viol"), f("evict_skip"), f("verdicts"),
            f("refusals"), f("queue_drop"), f("p1_viol")))
    comps, undet_arms = [], set()

    def point_comparison(bn, k, scope):
        def run(newn, ns):
            nx, ny = arr(newn, "share_any", ns), arr(newn, k, ns)
            kk = merge_points(nx, ny, BETTER[k], xnoise)
            bx, by = arr([bn], "share_any", ns)[0], arr([bn], k, ns)[0]
            rd = compare(nx, ny, kk, bx, by, k, nf_of(nf, k), xnoise, units, float(by.mean()), True)
            return rd, kk, nx, bx
        rd, kk, nx, bx = run(have, SEEDS)
        arms = {bn} | {have[i] for i in bracket(nx.mean(1), kk, float(bx.mean()))}
        note = ""
        if rd["word"] == "undetermined":
            undet_arms.update(arms)
            n24 = [n for n in have if full(n, 2 * SEEDS)]
            if full(bn, 2 * SEEDS) and len(n24) >= 2:                  # the registered doubling, on these arms only
                rd, _, _, _ = run(n24, 2 * SEEDS)
                rd["note"] = "doubled to %d seeds" % (2 * SEEDS)
        comps.append(dict(scope=scope, kind="point", base=bn, k=k, rd=rd))
        return rd

    P("\n### New model's H = 0 curve (T sweep) against each baseline point\n")
    P("Paired difference (the new curve interpolated per seed at that seed's baseline share, minus the baseline), signed so that positive favours the new model, against m = max(T_bar, A/A floor). A baseline below the curve's lowest delivered share is compared against the lowest point (amendment 1.6c). Today's Realtime rows are reported only, not under the verdict (1.6a).\n")
    P("| baseline | readout | difference | outcome | under the verdict |")
    P("|---|---|---|---|---|")
    for bn, scope in [(n_adr(r), "verdict") for r in RHO_SET] + [(n_rt(d), "reported") for d in DELTA_SET]:
        if not full(bn, SEEDS):
            continue
        for k in READ:
            rd = point_comparison(bn, k, scope)
            P("| %s | %s | %s | %s | %s |" % (bn, k, fmt(rd), rd["word"], "yes" if scope == "verdict" else "no, reported only"))
    P("\n### Curve against curve: the new model's points inside ADR-0084's range, against ADR-0084's interpolated rho curve\n")
    adr = [n for n in (n_adr(r) for r in RHO_SET) if full(n, SEEDS)]
    if len(adr) >= 2:
        for k in READ:
            def run(newn, adrn, ns, j):
                ax, ay = arr(adrn, "share_any", ns), arr(adrn, k, ns)
                ak = merge_points(ax, ay, BETTER[k], xnoise)
                px, py = arr([newn], "share_any", ns)[0], arr([newn], k, ns)[0]
                return compare_rev(ax, ay, ak, px, py, k, nf_of(nf, k), units, float(ay.mean())), ax, ak
            ax, ay = arr(adr, "share_any", SEEDS), arr(adr, k, SEEDS)
            ak = merge_points(ax, ay, BETTER[k], xnoise)
            lo, hi = ax[ak].mean(1).min(), ax[ak].mean(1).max()
            for n in have:
                x = float(arr([n], "share_any", SEEDS).mean())
                if not (lo - 1e-12 <= x <= hi + 1e-12):
                    continue
                rd, ax_, ak_ = run(n, adr, SEEDS, 0)
                arms = {n} | {adr[i] for i in bracket(ax_.mean(1), ak_, x)}
                if rd["word"] == "undetermined":
                    undet_arms.update(arms)
                    a24 = [m for m in adr if full(m, 2 * SEEDS)]
                    if full(n, 2 * SEEDS) and len(a24) >= 2:
                        rd, _, _ = run(n, a24, 2 * SEEDS, 0)
                        rd["note"] = "doubled to %d seeds" % (2 * SEEDS)
                comps.append(dict(scope="verdict", kind="curve", base=n, k=k, rd=rd))
                P("- new %s against ADR-0084's curve, %s: %s" % (n, k, fmt(rd)))
    if len(adr) >= 2 and not any(c["kind"] == "curve" for c in comps):
        P(empty_curve_note([float(arr([n], "share_any", SEEDS).mean()) for n in adr], [float(arr([n], "share_any", SEEDS).mean()) for n in have], xnoise))
    P("\n### Gates and identity\n")
    bad = []
    for n in allnames:
        for s in range(SEEDS):
            if s not in rows.get(n, {}):
                continue
            r = rows[n][s]
            if r.get("finished") is False or r["gauges"]["commit_viol"] or r["gauges"]["l1_viol"]:
                bad.append((n, s))
    P("- runs failing a gate (unfinished, commitment violation, L1 violation): %d" % len(bad))
    for x in bad[:20]:
        P("  - %s seed %d" % x)
    import summarize3c as S3
    idn = n_lt(INF, INF)
    gates_bad = bool(bad)
    if full(idn, SEEDS) and full(N_BULK, SEEDS):
        nd = 0
        for s in range(SEEDS):
            for k in S3.ID_KEYS:
                x, y = rows[idn][s].get(k), rows[N_BULK][s].get(k)
                if x != y and not (isinstance(x, float) and isinstance(y, float) and math.isnan(x) and math.isnan(y)):
                    nd += 1
        P("- identity, lifetime T=inf with hold inf against today's Bulk (C3), %d keys x %d seeds: %d differences" % (len(S3.ID_KEYS), SEEDS, nd))
        gates_bad = gates_bad or nd > 0
    out, worse, undet, better = classify(comps, gates_bad)
    P("\n### Outcome against the pass rule (this row, ADR-0084's rho curve only)\n")
    P("- comparisons under the verdict: %d; shown worse or outside the swept range: %d; undetermined: %d; shown better: %d" % (
        sum(1 for c in comps if c["scope"] == "verdict"), len(worse), len(undet), len(better)))
    for c in worse:
        P("  - WORSE/OUTSIDE: %s | %s | %s" % (c["base"], c["k"], c["rd"]["word"]))
    for c in undet:
        P("  - UNDETERMINED: %s | %s%s" % (c["base"], c["k"], " (after doubling)" if c["rd"].get("note") else " (12 seeds)"))
    P("- rule 2 (above somewhere): %s" % ("yes" if better else "no, in this row"))
    P("- **ROW %s**" % out)
    if a.write_undet:
        json.dump(sorted(undet_arms), open(a.write_undet, "w"))
    print("\n".join(L))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("cmd", choices=("list", "run", "freeze", "read"))
    ap.add_argument("--part", default="DECIDE")
    ap.add_argument("--seeds", type=int, default=SEEDS)
    ap.add_argument("--workers", type=int, default=1)
    ap.add_argument("--force", action="store_true")
    ap.add_argument("--seed0", type=int, default=0)
    ap.add_argument("--arms-file", default="")
    ap.add_argument("--nseeds", type=int, default=SEEDS)
    ap.add_argument("--write-undet", default="")
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
