"""The confirmation chain (three-knobs-revision4.md 12): builds A and B against today's designs under the clamp and elig = "rack".

    python confirm3e.py [--workers 9] [--only CELL] [--smoke]       one process, one pool; every part writes its own markers under out3e/confirm/

Order (12.5): the A/A rows, CL-long (the deciding case), then CL, CLw-long, CLc, CLc-v, CL-long-v, then OV-long and OV-long-v.  Each cell is gated, read and
doubled (once) before the next starts.  No stored row of any earlier stage is reused: runs are keyed by fingerprint under out3e/confirm/runs, a directory of
their own, and every fingerprint carries the clamp, the floor repeat and the eligibility fields.

Markers (all under out3e/confirm/): DONE-AA, DONE-<cell>, STOP (any gate), DONE-CONFIRM.  A STOP ends the process; nothing here clears one.
"""
import argparse
import dataclasses
import hashlib
import json
import math
import multiprocessing as mp
import os
import sys
import time

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import batch3b  # noqa: E402
import batch3e as B  # noqa: E402
import confirm_read as CR  # noqa: E402
import toy2  # noqa: E402

INF = math.inf
OUT = os.environ.get("CONFIRM_OUT", os.path.join(HERE, "out3e", "confirm"))
RESULTS = os.environ.get("CONFIRM_RESULTS", os.path.join(HERE, "results-confirm3e.md"))
ORDER = ("CL-long", "CL", "CLw-long", "CLc", "CLc-v", "CL-long-v", "OV-long", "OV-long-v")
CELLS12 = ("CL", "CL-long", "CLw-long", "CLc", "CLc-v", "CL-long-v")
OVER = ("OV-long", "OV-long-v")
SEEDS_HEAD, SEEDS_OVER, SEEDS_END, SEEDS_B = 12, 6, 3, 3
B_T = {"CLc": (0.4, 1.0, INF), "CLc-v": (1.0, 2.0, INF), "CL-long-v": (1.0, 2.0, INF)}          # the identity check of B against A in a headroom cell (12.1)
B_T_DEFAULT = (0.4, 2.0, INF)
RT_DELTAS = {"OV-long": (0.5, 50.0), "OV-long-v": (0.005, 50.0)}                              # 12.5: the old senders of the overload cells at their dial ends
SMOKE = {}                                                                                      # set by --smoke: {"scale": units fraction, "seeds": n}


# ---- the registry --------------------------------------------------------------------------------------------------------------------
class CJob:
    __slots__ = ("name", "cell", "design", "key", "nseeds", "builder", "trace_seeds", "fp")

    def __init__(self, name, cell, design, key, nseeds, builder, trace_seeds=0):
        if SMOKE:
            nseeds = min(nseeds, SMOKE["seeds"])
            trace_seeds = min(trace_seeds, nseeds)
        self.name, self.cell, self.design, self.key, self.nseeds, self.builder, self.trace_seeds = name, cell, design, key, nseeds, builder, trace_seeds
        self.fp = B.fingerprint(builder(0))


def common():
    return dict(clamp=True, elig="rack", rev4_gauge=True)


def smoke_fix(cfg):
    """--smoke only: a fraction of the units (the outage and the rates stay), for exercising the chain."""
    if SMOKE:
        n = max(int(cfg.n_total * SMOKE["scale"]), 2000)
        cfg = dataclasses.replace(cfg, n_total=n, t_limit=min(cfg.t_limit, 40.0))
    return cfg


def mk(cell, key, kw, rt_cwnd=False):
    base = B.base_of(cell)
    kw = dict(common(), **kw)
    if rt_cwnd:
        kw["cwnd_sim"] = float(B.CELLS[base][1]["pause_B"])                  # today's Realtime has no store pause: cwnd_sim = the cell's B (3.3)
    rb = B.row_builder(cell, key, "RL", **kw)
    return lambda s: smoke_fix(rb(s))


def sweep(cell):
    T, Hs = B.SWEEP[cell]
    if SMOKE:                                                                                   # --smoke: three lifetimes, T = inf with and without the hold
        T = [t for t in T if t in B_T.get(cell, B_T_DEFAULT)]
        Hs = [INF]
    return [("lt", t, 0.0) for t in T] + [("lt", t, INF) for t in Hs]


def seeds_head(cell):
    return SEEDS_HEAD if cell in CELLS12 else SEEDS_OVER


def jobs_of(cell):
    """Every row of one cell: build A's sweep, build B (3 seeds at three lifetimes in a headroom cell, the whole sweep in an overload cell), ADR-0084, today's Realtime
    (stored and engine-queue arms), today's Bulk."""
    out = []
    ns = seeds_head(cell)
    n_old = SEEDS_END if cell in OVER else SEEDS_HEAD
    a_kw = dict(floor_repeat="evidenced_overdue")
    b_set = set(B_T.get(cell, B_T_DEFAULT))
    for key in sweep(cell):
        ident_row = cell not in OVER and key[0] == "lt" and key[2] == 0.0 and key[1] in b_set
        gate_row = key == ("lt", INF, INF)
        out.append(CJob("%s | A | %s" % (cell, B.row_name(key)), cell, "A", key, ns, mk(cell, key, a_kw), trace_seeds=ns if gate_row else (SEEDS_B if ident_row else 0)))
    if cell in OVER:
        for key in sweep(cell):
            out.append(CJob("%s | B | %s" % (cell, B.row_name(key)), cell, "B", key, ns, mk(cell, key, dict(a_kw, wait_limit=True))))
    else:
        for t in sorted(b_set, key=lambda x: (x == INF, x)):
            key = ("lt", t, 0.0)
            out.append(CJob("%s | B | %s" % (cell, B.row_name(key)), cell, "B", key, SEEDS_B, mk(cell, key, dict(a_kw, wait_limit=True)), trace_seeds=SEEDS_B))
    rhos = B.OLD_ENDS[cell]["adr"] if cell in OVER else B.RHO_SET
    for r in rhos:
        out.append(CJob("%s | adr | %s" % (cell, B.row_name(("adr", r))), cell, "adr", ("adr", r), n_old, mk(cell, ("adr", r), {})))
    deltas = RT_DELTAS[cell] if cell in OVER else B.DELTA_SET + ([0.005] if cell in B.VIDEO else [])
    for d in deltas:
        out.append(CJob("%s | rt | %s" % (cell, B.row_name(("rt", d))), cell, "rt", ("rt", d), n_old, mk(cell, ("rt", d), {}, rt_cwnd=True)))
        out.append(CJob("%s | rteq | %s (engine queue)" % (cell, B.row_name(("rt", d))), cell, "rteq", ("rt", d), n_old, mk(cell, ("rt", d), dict(eq=True), rt_cwnd=True)))
    out.append(CJob("%s | bulk | %s" % (cell, B.row_name(("bulk",))), cell, "bulk", ("bulk",), n_old, mk(cell, ("bulk",), {}), trace_seeds=ns))
    return out


def aa_jobs():
    """A/A rows: build A at the cell's primary lifetime against its phase variant 1, 12 seeds in every cell (12.3).  The reference rows (variant 0, the sweep's own row)
    run here at 12 seeds too, so an overload cell's floor at 6 and 12 seeds exists before any comparison."""
    out = []
    for cell in ORDER:
        base, d, T = B.base_of(cell), B.delta_of(cell), B.primary_T(cell)
        for var in (0, 1):
            kw = dict(common(), floor_repeat="evidenced_overdue")
            out.append(CJob("%s | AA%s | build A at T=%s" % (cell, "'" if var else "", B.TAG(T)), cell, "aa", ("aa", var), SEEDS_HEAD,
                            (lambda s, base=base, d=d, T=T, var=var, kw=kw: smoke_fix(B.new_cfg(base, s, T, 0.0, d, B.rp("RL", "worth"), variant=var, **kw)))))
    return out


def registry_counts():
    """Runs per cell as 12.5 counts them (A/A' only in the A/A row)."""
    c = {cell: sum(j.nseeds for j in jobs_of(cell)) for cell in ORDER}
    aa = sum(j.nseeds for j in aa_jobs() if j.key == ("aa", 1))
    return c, aa


# ---- storage ----------------------------------------------------------------------------------------------------------------------------
def run_path(fp, seed):
    return os.path.join(OUT, "runs", "%s-%d.json" % (fp, seed))


def have(fp, seed):
    return os.path.exists(run_path(fp, seed))


def load(fp, seed):
    p = run_path(fp, seed)
    if not os.path.exists(p):
        return None
    with open(p) as f:
        return json.load(f)


def store(fp, seed, obj):
    p = run_path(fp, seed)
    os.makedirs(os.path.dirname(p), exist_ok=True)
    with open(p + ".tmp", "w") as f:
        json.dump(obj, f)
    os.replace(p + ".tmp", p)


# ---- one run ----------------------------------------------------------------------------------------------------------------------------
def tau_of_cell(cell):
    return B.tau_of(cell)


def run_job(job, seed):
    cfg = job.builder(seed)
    traced = seed < job.trace_seeds
    cfg = dataclasses.replace(cfg, want_trace=traced)
    sm = toy2.Sim(cfg)
    r = sm.run()
    extra = {}
    if traced:
        h = hashlib.sha256()
        for e in r["_trace"]:
            h.update(repr(e).encode())
        extra = dict(trace_sha=h.hexdigest(), trace_n=len(r["_trace"]))
    n = sm.n_sent
    tau = tau_of_cell(job.cell)
    ser = CR.make_series(sm.h_seq[:n], sm.dlv[:n], sm.units_total, sm.dt)
    r = batch3b.clean(r)
    r.update(extra)
    r["unfinished"] = int(r.get("finished") is False)
    r["delivered_by_t_limit"] = r["share_any"]
    r["series"] = ser
    r["confirm_design"] = job.design
    r["last_hand"] = (sm.units_total - 1) * sm.dt
    r["units_total"] = sm.units_total
    return r


PEAKS = {}


def need_gb(job):
    peak = PEAKS.get((job.cell, {"A": "new", "B": "new", "aa": "new", "rteq": "rt"}.get(job.design, job.design)), B.DEFAULT_PEAK_MB)
    return B.need_gb(peak)


def wait_mem(need, tag):
    n = 0
    t0 = time.time()
    while B.free_gb() < need:
        if n % 12 == 0:
            print("[%s] waiting for memory: %.1f GB free, %.1f needed (%s, waited %.0f s)" % (time.strftime("%H:%M:%S"), B.free_gb(), need, tag, time.time() - t0), flush=True)
        time.sleep(5.0)
        n += 1


JOBS_BY_FP = {}


def work(task):
    fp, seed = task
    job = JOBS_BY_FP[fp]
    wait_mem(need_gb(job), job.name)
    t = time.time()
    r = run_job(job, seed)
    r["wall_s"] = time.time() - t
    store(fp, seed, r)
    return fp, seed, r["wall_s"]


def init_worker(reg):
    global OUT
    OUT = reg["out"]
    SMOKE.update(reg["smoke"])
    for j in all_jobs():
        JOBS_BY_FP[j.fp] = j
    PEAKS.update(load_peaks())


def load_peaks():
    p = os.path.join(B.OUT, "time.json")
    if not os.path.exists(p):
        return {}
    try:
        with open(p) as f:
            return {(c, d): pk for c, d, sec, pk in json.load(f)["rows"]}
    except (OSError, ValueError, KeyError):
        return {}


def all_jobs():
    return aa_jobs() + [j for c in ORDER for j in jobs_of(c)]


def run_pool(pool, tasks, tag):
    """tasks: [(fp, seed)] not yet stored; heaviest classes first.  Returns the number run."""
    tasks = [t for t in tasks if not have(*t)]
    heavy = lambda t: -(JOBS_BY_FP[t[0]].cell in OVER) * 10 - (JOBS_BY_FP[t[0]].design == "adr") * 5   # noqa: E731
    tasks.sort(key=heavy)
    t0 = time.time()
    done = 0
    for fp, seed, w in pool.imap_unordered(work, tasks, chunksize=1):
        done += 1
        if done % 25 == 0 or done == len(tasks):
            print("[%s] %s: %d/%d runs (%.0f s)" % (time.strftime("%H:%M:%S"), tag, done, len(tasks), time.time() - t0), flush=True)
    return done


def tasks_of(jobs, lo=0, hi=None):
    return [(j.fp, s) for j in jobs for s in range(lo, j.nseeds if hi is None else min(hi, j.nseeds))]


# ---- gates ------------------------------------------------------------------------------------------------------------------------------
def stop(text):
    os.makedirs(OUT, exist_ok=True)
    with open(os.path.join(OUT, "STOP"), "w") as f:
        f.write(text + "\n")
    print("STOP: " + text, flush=True)


def stopped():
    return os.path.exists(os.path.join(OUT, "STOP"))


def ident_diff(a, b):
    """id3e keys and the trace digest of two runs, without the blocks that differ by construction (the rev4 echo of the build's flags, the stored series, timings)."""
    skip = {"rev4", "series", "wall_s", "confirm_design", "resend_outcome"}
    a2 = {k: v for k, v in a.items() if k not in skip}
    b2 = {k: v for k, v in b.items() if k not in skip}
    bad = list(B.id3e(a2, b2))
    if a.get("trace_sha") != b.get("trace_sha"):
        bad.append("trace")
    return bad


ABSOLUTE = ("rep_fire_viol_carrier",)


def run_gates(cell, jobs):
    """12.1's stop rules for one cell: every new-model run finishes; the per-run absolutes; B = A in a headroom cell (traces, id3e keys, n_tau_lt_T = 0);
    S5 at T = inf, H = inf = today's Bulk (every key and trace).  Returns the list of failures; the caller writes the STOP."""
    bad = []
    byname = {j.name: j for j in jobs}
    for j in jobs:
        for s in range(j.nseeds):
            r = load(j.fp, s)
            if r is None:
                bad.append("%s seed %d: no run" % (j.name, s))
                continue
            g = r["gauges"]
            if j.design in ("A", "B", "aa") and r.get("finished") is False:
                bad.append("%s seed %d: unfinished" % (j.name, s))
            for k in ("l1_viol", "l1_viol_rep", "commit_viol", "worth_viol_rep"):
                if j.design in ("A", "B", "aa", "adr") and g.get(k, 0) and (k != "worth_viol_rep" or j.design in ("A", "B", "aa")):
                    bad.append("%s seed %d: %s = %s" % (j.name, s, k, g[k]))
            if j.design in ("A", "B", "aa"):
                for k in ABSOLUTE:
                    if g.get(k, 0):
                        bad.append("%s seed %d: %s = %s" % (j.name, s, k, g[k]))
                if r["rev4"]["elig_viol"]:
                    bad.append("%s seed %d: elig_viol = %s" % (j.name, s, r["rev4"]["elig_viol"]))
                if j.key[2:3] == (0.0,) and g.get("rep_supp_viol", 0):                 # H = 0 rows: the receiver's floor holds; H = inf rows (the hold) are reported only
                    bad.append("%s seed %d: rep_supp_viol = %s" % (j.name, s, g["rep_supp_viol"]))
    if cell not in OVER:                                                                # B = A, the identity check
        for j in jobs:
            if j.design != "B":
                continue
            a = byname.get(j.name.replace("| B |", "| A |"))
            for s in range(j.nseeds):
                ra, rb = load(a.fp, s), load(j.fp, s)
                if ra is None or rb is None:
                    continue
                d = ident_diff(ra, rb)
                if d:                                                                   # n_tau_lt_T and deq_early_would are reported per row, not a stop (2026-11-26, pre-data)
                    bad.append("B != A in %s at %s seed %d: %s" % (cell, j.name, s, d))
    sa = [j for j in jobs if j.design == "A" and j.key == ("lt", INF, INF)]
    bu = [j for j in jobs if j.design == "bulk"]
    for a, b in zip(sa, bu):
        for s in range(a.nseeds):
            ra, rb = load(a.fp, s), load(b.fp, s)
            if ra is None or rb is None:
                continue
            d = ident_diff(ra, rb)
            if d:
                bad.append("S5 at T = inf, H = inf != Bulk in %s seed %d: %s" % (cell, s, d))
    return bad


# ---- reading --------------------------------------------------------------------------------------------------------------------------
def recs(job, n=None):
    """The stored runs of a row, annotated with its cell's tau (a run is keyed by its configuration, so two cells can share one)."""
    out = []
    for s in range(job.nseeds if n is None else n):
        r = load(job.fp, s)
        if r is None:
            break
        r["tau"] = tau_of_cell(job.cell)
        out.append(r)
    return out


def dead_of(cell):
    return B.CELLS[B.base_of(cell)][1].get("dead")


def units_of(cell):
    return B.CELLS[B.base_of(cell)][1]["n_total"]


def nf_path():
    return os.path.join(OUT, "confirm_nf.json")


def freeze_nf():
    """The re-frozen A/A floors under build A (12.3): per cell and per seed count n (3, 6, 12), the p95 over the first n seeds of |A - A'| on the paired window."""
    nf = {}
    for cell in ORDER:
        a = [j for j in aa_jobs() if j.cell == cell and j.key == ("aa", 0)][0]
        aa = [j for j in aa_jobs() if j.cell == cell and j.key == ("aa", 1)][0]
        ra, rb = recs(a), recs(aa)
        nf[cell] = {str(n): CR.nf_from_aa(ra, rb, dead_of(cell), n) for n in (3, 6, 12) if min(len(ra), len(rb)) >= n}
    os.makedirs(OUT, exist_ok=True)
    with open(nf_path(), "w") as f:
        json.dump(nf, f, indent=1)
    return nf


def nf_for(nf, cell, n):
    have_n = sorted(int(k) for k in nf[cell])
    use = max([k for k in have_n if k <= n] or [have_n[0]])
    return nf[cell][str(use)]


def steady_arrays(cell, new_recs, base_recs, n):
    """Per seed paired readouts of a row against a baseline row: arrays share, ot, p50 (and the baseline's)."""
    dead = dead_of(cell)
    xs = {k: [] for k in ("share", "ot", "p50")}
    ys = {k: [] for k in ("share", "ot", "p50")}
    for s in range(n):
        pr = CR.paired(new_recs[s], base_recs[s], dead)
        if pr is None:                                                                      # an empty paired window: NaN, read as "no steady window" by the caller
            for k in xs:
                xs[k].append(math.nan)
                ys[k].append(math.nan)
            continue
        for k in xs:
            xs[k].append(pr[0][k])
            ys[k].append(pr[1][k])
    return {k: np.asarray(v) for k, v in xs.items()}, {k: np.asarray(v) for k, v in ys.items()}


def identity_row(a_job, b_job, n):
    """Bulk (and the object API) read by identity against S5 at T = inf, H = inf (stage 3e's rule): the two rows are the same run on every id3e key and trace, at every seed."""
    bad = []
    for s in range(n):
        ra, rb = load(a_job.fp, s), load(b_job.fp, s)
        if ra is None or rb is None:
            return None
        d = ident_diff(ra, rb)
        if d:
            bad.append((s, d))
    return bad


def read_cell(cell, jobs, nf, double=frozenset()):
    try:
        return _read_cell(cell, jobs, nf, double)
    except ValueError as e:                                                                  # an empty steady window is a finding, not a crash
        return ["### " + cell, "No reading: %s" % e], set()


def _read_cell(cell, jobs, nf, double=frozenset()):
    """The cell's reading as markdown lines and the arms of every undetermined comparison (the registered doubling).  Matched share where the build's T sweep spans a
    range of steady delivered shares (outage and headroom cells), dominance in the overload cells.  The H = inf rows, B in a headroom cell (an identity check) and the
    rows the verdict does not use are reported in the table only."""
    L = []
    P = L.append
    units = units_of(cell)
    video = cell in B.VIDEO
    A = [j for j in jobs if j.design == "A" and j.key[0] == "lt" and j.key[2] == 0.0]
    base = [j for j in jobs if j.design in ("adr", "rt", "rteq", "bulk")]
    Bs = [j for j in jobs if j.design == "B" and j.key[2] == 0.0]
    undet = set()
    P("\n### %s\n" % cell)
    P("Builds: A = Revision 3 + clamp + floor repeat (evidenced_overdue) + elig rack; B = A + wait limit, no gate. Steady window per comparison, paired per seed (12.2). %s\n" % (
        "Delay p50 is reported only here (delta = 0.005, 12.3)." if video else ""))
    if cell == "CL-long":
        P("Labelled post-data: build A's CL-long rows (and the clamp rows of the new model there) were sized in step 2 (three-knobs-revision4.md 9.1) before this registration; they confirm nothing. What is new here is the baselines under the clamp and elig = rack.")
    P("| row | seeds | whole-run delivered | on time within %gs | recovery (s) | elig_R_bind | floors | repeats | resends (dup) | unfinished | n_tau_lt_T (ticks, mean) | deq_early_would (mean) |" % tau_of_cell(cell))
    P("|---|---|---|---|---|---|---|---|---|---|---|---|")
    for j in jobs:
        rs = recs(j)
        if not rs:
            continue
        m = lambda f: float(np.mean([f(r) for r in rs]))                                     # noqa: E731
        ot = m(lambda r: sum(r["series"]["ot"][CR.tkey(r["tau"])]) / max(sum(r["series"]["handed"]), 1))
        rec = [r.get("recovery_time") for r in rs]
        P("| %s | %d | %.4f | %.4f | %s | %.3f | %.0f | %.0f | %.0f (%.0f) | %d | %.0f | %.0f |" % (
            cell_label(j), len(rs), m(lambda r: r["share_any"]), ot, ("%.2f" % np.mean([REC if x is None else x for x in rec for REC in (CR.REC_CAP,)])) if dead_of(cell) else "-",
            m(lambda r: r["rev4"]["elig_R_bind"] / max(r["rev4"]["elig_n"], 1)), m(lambda r: r["gauges"].get("floors", 0)),
            m(lambda r: (r["gauges"].get("rep_fired_by_knowledge") or 0) + (r["gauges"].get("rep_fired_by_timeout") or 0)),
            m(lambda r: r["gauges"].get("repair_sends", 0)), m(lambda r: r["gauges"].get("dup", 0)), sum(r["unfinished"] for r in rs),
            m(lambda r: r["rev4"]["n_tau_lt_T"]), m(lambda r: r["rev4"]["deq_early_would"])))
    if cell not in OVER:
        outage = dead_of(cell) is not None
        P("\nMatched share. %s The new curve is build A's H = 0 sweep, interpolated per seed at the baseline's share, against the baseline; a baseline below the curve's "
          "lowest share reads the lowest point. Positive favours build A. Bars: shares max(0.05 (1 - ref), 1/units, NF), p50 max(NF_p50, 0.05 ref), recovery max(0.05 ref, 0.1 s, NF).\n" % (
              "Outage cell: the axis is the whole-run delivered share and the readouts are the whole-run share on time and the registered recovery time, as in stage 3e; the steady p50 is "
              "on the paired window (outage and recovery excluded)." if outage else "Headroom cell: the axis is the steady delivered share on the paired window."))
        P("| baseline | readout | difference [95 %] | bar | outcome |")
        P("|---|---|---|---|---|")
        readouts = (("whole-run on time", "ot", 1.0), ("recovery", "recovery", -1.0), ("steady p50", "p50", -1.0)) if outage else (("steady on time", "ot", 1.0), ("steady p50", "p50", -1.0))
        sinf = [j for j in jobs if j.design == "A" and j.key == ("lt", INF, INF)]
        for bj in base:
            n = min([len(recs(bj))] + [len(recs(a)) for a in A])
            if bj.name not in double and n > bj.nseeds:
                n = bj.nseeds
            nfv = nf_for(nf, cell, n)
            brecs = recs(bj, n)
            if bj.design == "bulk" and sinf:                                              # read by identity, not on the curve (a strict-order row sits outside the any-order H = 0 curve)
                bad = identity_row(sinf[0], bj, n)
                if bad is not None:
                    P("| %s | all readouts | identity against S5 at T=inf, H=inf: %s | - | %s |" % (cell_label(bj), "identical on every key and trace at %d seeds" % n if not bad else "differs at seeds %s" % [b[0] for b in bad],
                                                                                              "equivalent (identity)" if not bad else "NOT identical"))
                    continue
            for name, key, sign in readouts:
                px, py = [], []
                for a in A:
                    ar = recs(a, n)
                    xs, ys = steady_arrays(cell, ar, brecs, n)
                    if outage:
                        px.append(np.asarray([CR.whole(r["series"], r["tau"])[0] for r in ar]))
                        py.append({"ot": np.asarray([CR.whole(r["series"], r["tau"])[1] for r in ar]), "recovery": np.asarray([rec_of(r) for r in ar]), "p50": xs["p50"]}[key])
                    else:
                        px.append(xs["share"])
                        py.append(xs[key])
                px, py = np.stack(px), np.stack(py)
                xs0, ys0 = steady_arrays(cell, recs(A[0], n), brecs, n)
                if key == "p50" and (np.isnan(py).any() or np.isnan(ys0["p50"]).any()):
                    P("| %s | %s | no steady window: the paired window of 12.2 is empty at %d of %d seeds for at least one build-A point (an arm has not recovered within the run, so the outage exclusion of 12.2 runs to the end) | - | not read (registration finding) |" % (
                        cell_label(bj), name, int((np.isnan(py).any(axis=0) | np.isnan(ys0["p50"])).sum()), n))
                    continue
                if outage:
                    bx = np.asarray([CR.whole(r["series"], r["tau"])[0] for r in brecs])
                    by = {"ot": np.asarray([CR.whole(r["series"], r["tau"])[1] for r in brecs]), "recovery": np.asarray([rec_of(r) for r in brecs]), "p50": ys0["p50"]}[key]
                    xnoise = max(nfv["wshare"], 1.0 / units)
                else:
                    bx, by = ys0["share"], ys0[key]
                    xnoise = max(nfv["share"], 1.0 / units)
                kk = merge_for(px, py, sign, xnoise)
                ref = float(np.mean(by[np.isfinite(by)])) if np.isfinite(by).any() else math.inf
                if key == "ot":
                    bar = CR.share_bar(ref, units, nfv["wot"] if outage else nfv["ot"])
                elif key == "p50":
                    bar = CR.p50_bar(ref, nfv["p50"])
                else:
                    bar = CR.rec_bar(ref, nfv["recovery"])
                rd = CR.compare_curve(px, py, kk, bx, by, sign, bar)
                if rd["word"] == "undetermined" and bj.name not in double:
                    undet.add(bj.name)
                    undet.update(a.name for a in A)
                note = " (p50 reported only)" if (video and key == "p50") else ""
                P("| %s | %s | %s | %.3g | %s%s |" % (cell_label(bj), name, fmt(rd), bar, rd["word"], note))
    else:
        P("\nDominance (12.3): each baseline against the build's best finite-T point per seed pairing (the highest seed-mean steady on-time share; ties within its bar broken by the lower p50, then by the smaller T). "
          "Dominates = not worse beyond the bar on any readout and better beyond it on at least one.\n")
        P("| build | baseline | best point | share | on time | p50 | outcome |")
        P("|---|---|---|---|---|---|---|")
        for build, rows in (("A", A), ("B", Bs)):
            fin = [j for j in rows if j.key[1] != INF]
            for bj in base:
                n = min([len(recs(bj))] + [len(recs(j)) for j in fin])
                if bj.name not in double and n > bj.nseeds:
                    n = bj.nseeds
                nfv = nf_for(nf, cell, n)
                pts = []
                for j in fin:
                    xs, ys = steady_arrays(cell, recs(j, n), recs(bj, n), n)
                    bar_ot = CR.share_bar(float(ys["ot"].mean()), units, nfv["ot"])
                    pts.append((j, j.key[1], xs, bar_ot, ys))
                best = CR.best_point([(p[0], p[1], p[2], p[3]) for p in pts])
                pt = [p for p in pts if p[0] is best[0]][0]
                xs, ys = pt[2], pt[4]
                rds = {"share": CR.compare_pair(xs["share"], ys["share"], 1.0, CR.share_bar(float(ys["share"].mean()), units, nfv["share"])),
                       "ot": CR.compare_pair(xs["ot"], ys["ot"], 1.0, CR.share_bar(float(ys["ot"].mean()), units, nfv["ot"])),
                       "p50": CR.compare_pair(xs["p50"], ys["p50"], -1.0, CR.p50_bar(float(np.mean(np.where(np.isfinite(ys["p50"]), ys["p50"], 1e9))), nfv["p50"]))}
                if video:
                    rds_v = {k: v for k, v in rds.items() if k != "p50"}
                    word = CR.dominance_word(rds_v)
                else:
                    word = CR.dominance_word(rds)
                if word == "undetermined" and bj.name not in double:
                    undet.add(bj.name)
                    undet.add(pt[0].name)
                P("| %s | %s | T=%s | %s | %s | %s | %s%s |" % (build, cell_label(bj), B.TAG(pt[1]), fmt(rds["share"]), fmt(rds["ot"]), fmt(rds["p50"]), word,
                                                              " (p50 reported only)" if video else ""))
    return L, undet


def rec_of(r):
    return CR.REC_CAP if r.get("recovery_time") is None else r["recovery_time"]


def cell_label(job):
    return job.name.split(" | ", 1)[1].replace(" | ", "; ")


def merge_for(px, py, sign, xnoise):
    finite = np.where(np.isfinite(py), py, -1e9 * sign)
    return merge_points(px, finite, sign, xnoise)


from proof_s1 import merge_points  # noqa: E402


def fmt(rd):
    if rd["word"].startswith("outside"):
        return "outside the swept range (%d seeds)" % rd.get("n_out", 0)
    return "%.3g [%.3g, %.3g]" % (rd["mean"], rd["lo"], rd["hi"])


# ---- the chain --------------------------------------------------------------------------------------------------------------------------
def append_results(lines):
    with open(RESULTS, "a", encoding="utf-8") as f:
        f.write("\n".join(lines) + "\n")


def marker(name):
    open(os.path.join(OUT, name), "w").close()


def done(name):
    return os.path.exists(os.path.join(OUT, name))


def run_chain(workers, only=None):
    os.makedirs(OUT, exist_ok=True)
    reg = dict(smoke=dict(SMOKE), out=OUT)
    for j in all_jobs():
        JOBS_BY_FP[j.fp] = j
    PEAKS.update(load_peaks())
    if stopped():
        print("STOP marker present: not starting", flush=True)
        return 1
    with mp.Pool(workers, initializer=init_worker, initargs=(reg,)) as pool:
        if not done("DONE-AA"):
            print("[%s] A/A rows" % time.strftime("%H:%M:%S"), flush=True)
            run_pool(pool, tasks_of(aa_jobs()), "AA")
            nf = freeze_nf()
            marker("DONE-AA")
            append_results(["\n## Confirmation (three-knobs-revision4.md 12): the A/A floors under build A (%s)\n" % time.strftime("%Y-%m-%d"), "```", json.dumps(nf, indent=1), "```"])
        nf = json.load(open(nf_path()))
        for cell in ORDER:
            if only and cell != only:
                continue
            if done("DONE-" + cell):
                continue
            jobs = jobs_of(cell)
            print("[%s] cell %s: %d runs" % (time.strftime("%H:%M:%S"), cell, sum(j.nseeds for j in jobs)), flush=True)
            run_pool(pool, tasks_of(jobs), cell)
            bad = run_gates(cell, jobs)
            if bad and SMOKE:                                                           # a smoke run reports its gate failures and goes on, so the read and the doubling execute
                print("smoke: gate failures (a real run stops here): %s" % "; ".join(bad[:5]), flush=True)
                append_results(["### " + cell + ": gate failures (smoke, not stopped)"] + ["- " + b for b in bad[:20]])
            elif bad:
                stop("cell %s: %s" % (cell, "; ".join(bad[:10])))
                append_results(["\n### %s: STOPPED by a gate\n" % cell] + ["- " + b for b in bad[:40]])
                return 1
            lines, undet = read_cell(cell, jobs, nf)
            if undet:                                                                    # the one registered doubling, on the arms of the undetermined comparisons
                dj = [j for j in jobs if j.name in undet]
                extra = []
                for j in dj:
                    n2 = min(2 * j.nseeds, 24)
                    j2 = CJob(j.name, j.cell, j.design, j.key, n2, j.builder, j.trace_seeds)
                    extra += [(j.fp, s) for s in range(j.nseeds, n2)]
                print("[%s] cell %s: doubling %d arms (%d runs)" % (time.strftime("%H:%M:%S"), cell, len(dj), len(extra)), flush=True)
                run_pool(pool, extra, cell + " doubling")
                for j in dj:
                    j.nseeds = min(2 * j.nseeds, 24)
                lines, undet2 = read_cell(cell, jobs, nf, double=frozenset(undet))
            append_results(lines)
            marker("DONE-" + cell)
    if not only:
        marker("DONE-CONFIRM")
    return 0


def main(argv=None):
    ap = argparse.ArgumentParser()
    ap.add_argument("--workers", type=int, default=9)
    ap.add_argument("--only", default=None)
    ap.add_argument("--smoke", action="store_true", help="exercise the whole chain on a fraction of the units, 3 seeds, in a scratch out dir")
    ap.add_argument("--counts", action="store_true")
    ap.add_argument("--smoke-out", default=None, help="reuse a smoke directory")
    ap.add_argument("--time-one", action="store_true", help="12.5's timing run: one seed of ADR-0084 at rho 0.95 in OV-long-v under the clamp and elig rack; prints wall time and peak memory")
    a = ap.parse_args(argv)
    if a.counts:
        c, aa = registry_counts()
        print(c, aa, sum(c.values()) + aa)
        return 0
    B.build()
    if a.time_one:
        import psutil
        job = [j for j in jobs_of("OV-long-v") if j.design == "adr" and j.key == ("adr", 0.95)][0]
        proc = psutil.Process()
        peak = [0]
        import threading

        def watch():
            while True:
                peak[0] = max(peak[0], proc.memory_info().rss)
                time.sleep(0.2)
        threading.Thread(target=watch, daemon=True).start()
        t = time.time()
        r = run_job(job, 0)
        print("timing run: %s seed 0: wall %.1f s, peak %.0f MB, finished %s, delivered %.4f" % (job.name, time.time() - t, peak[0] / 2.0 ** 20, r.get("finished"), r["share_any"]))
        return 0
    if a.smoke:
        import tempfile
        global OUT, RESULTS
        SMOKE.update(scale=0.75, seeds=3)
        OUT = a.smoke_out or tempfile.mkdtemp(prefix="confirm_smoke_")
        RESULTS = os.path.join(OUT, "results-smoke.md")
        print("smoke run in", OUT, flush=True)
    return run_chain(min(a.workers, 9), a.only)


if __name__ == "__main__":
    mp.freeze_support()
    sys.exit(main())
