"""The build B' confirmation (three-knobs-revision4.md 13.6, 13.7, 13.8): the re-derived wait limit against build A, build B and today's designs.

    python confirm13.py [--workers 9] [--only PART] [--smoke] [--counts]

It reuses confirm3e.py's machinery (the registry, the store, the worker, the readers) and leaves confirm3e.py's stored-job fingerprints alone: stored A, old-sender,
Bulk and B rows are read from out3e/confirm/runs where the configuration fingerprint matches; new runs go to the same store.  Parts, in order (each writes
DONE13-<part>; a gate writes STOP13 and ends the process):

    AA-m       the A/A floor of OV-long-m, 2 x 12 seeds
    IDENT      the headroom identity (B' = A, trace for trace): CLc, CLc-v, CL-long-v at B_T, CL-long, CL, CLw-long at T in {0.4, 2, inf}, seeds 0-2, plus the traced pair
               CL-long-v, T = 2, seed 11 (a rerun of A with the trace on, kept under its own key so the stored A row is untouched)
    OV-long-v  B' at the 8 rows of the sweep, 6 seeds; stop: the bill check
    OV-long    the same at delta = 50
    OV-long-m  the new cell (OV-long's link, load, units and tau at delta = 0.1): A and B' at 5 lifetimes x 6 seeds, today's Realtime at delta = 0.1 (both arms) and
               ADR-0084 / Bulk (3 seeds each, reused from OV-long when the fingerprint is equal); stop: onset_bind inert or saturated
    K-OV       A and B' at T = 0.4, 6 seeds, reported only

Readings follow 12.3 (dominance in the overload cells, with the reader fix) in results-confirm13.md.
"""
import argparse
import dataclasses
import json
import math
import multiprocessing as mp
import os
import sys
import time

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import batch3e as B  # noqa: E402
import confirm3e as C  # noqa: E402
import confirm_read as CR  # noqa: E402

INF = math.inf
RESULTS = os.environ.get("CONFIRM13_RESULTS", os.path.join(HERE, "results-confirm13.md"))
M, M_DELTA = "OV-long-m", 0.1
A_KW = dict(floor_repeat="evidenced_overdue")
BP_KW = dict(A_KW, wait_limit=True, wait_limit_form="rev4b")
IDENT_CELLS = ("CLc", "CLc-v", "CL-long-v", "CL-long", "CL", "CLw-long")
OV_CELLS = ("OV-long-v", "OV-long")
PARTS = ("AA-m", "IDENT", "OV-long-v", "OV-long", "OV-long-m", "K-OV")
BILL_STOP = 1900.0                                   # 13.6 stop 2: 0.004 x 480 000 units
SEED_X = 11                                          # 13.8: CL-long-v's live seed
MAX_UNITS = 24
SMOKE_SCALE, SMOKE_SEEDS = 0.25, 3


# ---- the new cell, registered at module level so that spawned workers see it too --------------------------------------------------------------
if M not in B.CELLS:
    B.CELLS[M] = B._copy(B.CELLS, "OV-long")           # OV-long's link, load, units and tau; only delta differs (13.7)
    B.SWEEP[M] = B.SWEEP["OV-long"]
    _delta_of = B.delta_of
    B.delta_of = lambda cell: M_DELTA if cell == M else _delta_of(cell)


# ---- the registry -------------------------------------------------------------------------------------------------------------------------
def bp_job(cell, key, nseeds, trace_seeds=0):
    return C.CJob("%s | Bp | %s" % (cell, B.row_name(key)), cell, "B", key, nseeds, C.mk(cell, key, BP_KW), trace_seeds=trace_seeds)


def is_bp(job):
    return " | Bp | " in job.name


def ov_new(cell):
    """B' in an overload cell: the whole sweep at 6 seeds; the T = inf, H = inf row is traced (the gate against Bulk)."""
    return [bp_job(cell, key, C.seeds_head(cell), trace_seeds=C.seeds_head(cell) if key == ("lt", INF, INF) else 0) for key in C.sweep(cell)]


def m_keys():
    return [k for k in C.sweep(M) if k[2] == 0.0]


def m_jobs():
    """OV-long-m: A and B' at the five lifetimes (H = 0), today's Realtime at delta = 0.1 (both arms), ADR-0084 at its dial ends and Bulk."""
    out = []
    for key in m_keys():
        out.append(C.CJob("%s | A | %s" % (M, B.row_name(key)), M, "A", key, 6, C.mk(M, key, A_KW)))
    out += [bp_job(M, key, 6) for key in m_keys()]
    d = ("rt", M_DELTA)
    out.append(C.CJob("%s | rt | %s" % (M, B.row_name(d)), M, "rt", d, C.SEEDS_END, C.mk(M, d, {}, rt_cwnd=True)))
    out.append(C.CJob("%s | rteq | %s (engine queue)" % (M, B.row_name(d)), M, "rteq", d, C.SEEDS_END, C.mk(M, d, dict(eq=True), rt_cwnd=True)))
    for r in B.OLD_ENDS["OV-long"]["adr"]:
        out.append(C.CJob("%s | adr | %s" % (M, B.row_name(("adr", r))), M, "adr", ("adr", r), C.SEEDS_END, C.mk(M, ("adr", r), {})))
    out.append(C.CJob("%s | bulk | %s" % (M, B.row_name(("bulk",))), M, "bulk", ("bulk",), C.SEEDS_END, C.mk(M, ("bulk",), {})))
    return out


def m_stored_fps():
    return {j.fp for j in C.jobs_of("OV-long")}


def aa_m():
    """A against its phase variant at T = 0.4, 12 seeds, in OV-long-m (the cell's own floor; the two stored cells keep 12.3's frozen ones)."""
    out = []
    T = B.primary_T(M)
    for var in (0, 1):
        out.append(C.CJob("%s | AA%s | build A at T=%s" % (M, "'" if var else "", B.TAG(T)), M, "aa", ("aa", var), C.SEEDS_HEAD,
                          (lambda s, var=var: C.smoke_fix(B.new_cfg(B.base_of(M), s, T, 0.0, M_DELTA, B.rp("RL", "worth"), variant=var, **dict(C.common(), **A_KW))))))
    return out


def ident_pairs():
    """(A job, B' job) per identity cell and lifetime: seeds 0-2, traced (A's stored rows carry the trace)."""
    out = []
    for cell in IDENT_CELLS:
        ts = set(C.B_T.get(cell, C.B_T_DEFAULT))
        a_by_t = {j.key[1]: j for j in C.jobs_of(cell) if j.design == "A" and j.key[0] == "lt" and j.key[2] == 0.0 and j.key[1] in ts}
        for t in sorted(ts, key=lambda x: (x == INF, x)):
            key = ("lt", t, 0.0)
            out.append((a_by_t[t], bp_job(cell, key, C.SEEDS_B, trace_seeds=C.SEEDS_B)))
    return out


def seed_x_pair():
    """CL-long-v at T = 2, seed 11 (13.8): a traced A rerun and a traced B', one run each.  Each is a one-seed job at seed 11, keyed apart from the stored A row (same fingerprint, so a suffix)."""
    cell, key = "CL-long-v", ("lt", 2.0, 0.0)
    out = []
    for tag, kw in (("A", A_KW), ("Bp", BP_KW)):
        base = C.mk(cell, key, kw)
        j = C.CJob("%s | %s | %s (traced, seed %d)" % (cell, tag, B.row_name(key), SEED_X), cell, "A" if tag == "A" else "B", key, 1, (lambda s, base=base: base(SEED_X)), trace_seeds=1)
        j.fp = j.fp + "t%d" % SEED_X
        out.append(j)
    return out


def kov_jobs():
    key = ("lt", 0.4, 0.0)
    return [C.CJob("K-OV | A | %s" % B.row_name(key), "K-OV", "A", key, 6, C.mk("K-OV", key, A_KW)), bp_job("K-OV", key, 6)]


def part_jobs():
    """The new runs of every part (stored rows are read, not listed here)."""
    ip = ident_pairs()
    return {
        "AA-m": aa_m() + ([j for j in C.aa_jobs() if j.cell in OV_CELLS] if C.SMOKE else []),      # a smoke run has no stored floors for the two old cells
        "IDENT": [b for _, b in ip] + seed_x_pair(),
        "OV-long-v": ov_new("OV-long-v"),
        "OV-long": ov_new("OV-long"),
        "OV-long-m": [j for j in m_jobs() if not (j.design in ("adr", "bulk") and j.fp in m_stored_fps())],
        "K-OV": kov_jobs(),
    }


def all_jobs13():
    out = [j for js in part_jobs().values() for j in js]
    out += [a for a, _ in ident_pairs()] + m_jobs()
    for c in OV_CELLS:
        out += C.jobs_of(c)
    return out


def init_worker13(reg):
    C.init_worker(reg)
    for j in all_jobs13():
        C.JOBS_BY_FP[j.fp] = j


def runs_of(jobs):
    return sum(j.nseeds for j in jobs)


def counts():
    pj = part_jobs()
    reg = {p: runs_of(js) for p, js in pj.items()}
    fresh_old = sum(j.nseeds for j in pj["OV-long-m"] if j.design in ("adr", "bulk"))
    stored = []
    for c in OV_CELLS:
        stored += C.jobs_of(c)
    stored += [a for a, _ in ident_pairs()]
    miss = sum(1 for j in stored for s in range(j.nseeds) if not C.have(j.fp, s))
    todo = sum(1 for js in pj.values() for j in js for s in range(j.nseeds) if not C.have(j.fp, s))
    return reg, fresh_old, miss, todo


# ---- gates ------------------------------------------------------------------------------------------------------------------------------
def stop13(text):
    os.makedirs(C.OUT, exist_ok=True)
    with open(os.path.join(C.OUT, "STOP13"), "w") as f:
        f.write(text + "\n")
    print("STOP: " + text, flush=True)


def stopped13():
    return os.path.exists(os.path.join(C.OUT, "STOP13"))


def absolutes(jobs):
    """12.1 / 13.6 stop 4: every new-model run finishes; the per-run absolutes hold."""
    bad = []
    for j in jobs:
        for s in range(j.nseeds):
            r = C.load(j.fp, s)
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
                if g.get("rep_fire_viol_carrier", 0):
                    bad.append("%s seed %d: rep_fire_viol_carrier = %s" % (j.name, s, g["rep_fire_viol_carrier"]))
                if r["rev4"]["elig_viol"]:
                    bad.append("%s seed %d: elig_viol = %s" % (j.name, s, r["rev4"]["elig_viol"]))
                if j.key[2:3] == (0.0,) and g.get("rep_supp_viol", 0):
                    bad.append("%s seed %d: rep_supp_viol = %s" % (j.name, s, g["rep_supp_viol"]))
    return bad


def gate_identity(pairs):
    """13.6 stop 1: B' = A on every key and the trace, at every seed.  Returns (failures, report rows)."""
    bad, rows = [], []
    for a, b in pairs:
        for s in range(b.nseeds):
            ra, rb = C.load(a.fp, s), C.load(b.fp, s)
            if ra is None or rb is None:
                bad.append("%s seed %d: no run" % (b.name, s))
                continue
            d = C.ident_diff(ra, rb)
            r4 = rb["rev4"]
            rows.append((b.cell, a.key[1], s, d, r4))
            if d:
                bad.append("B' != A in %s T=%s seed %d: %s" % (b.cell, B.TAG(a.key[1]), s, d))
    return bad, rows


def seed_x_diff():
    a, b = seed_x_pair()
    ra, rb = C.load(a.fp, 0), C.load(b.fp, 0)
    if ra is None or rb is None:
        return None, None
    return C.ident_diff(ra, rb), rb["rev4"]


def gate_s5(bp_rows, bulk_rows):
    """12.1: S5 at T = inf, H = inf equals Bulk on every key and trace, for B' as for A (the law is inert at T = inf)."""
    bad = []
    for b, u in zip(bp_rows, bulk_rows):
        for s in range(b.nseeds):
            rb, ru = C.load(b.fp, s), C.load(u.fp, s)
            if rb is None or ru is None:
                continue
            d = C.ident_diff(rb, ru)
            if d:
                bad.append("B' at T = inf, H = inf != Bulk in %s seed %d: %s" % (b.cell, s, d))
    return bad


def units_of_run(r):
    return r["units_total"]


def bill_table(cell, a_jobs, bp_jobs, b_jobs):
    """Whole-run delivered units, A - B' (and B' - B), per finite lifetime, mean over the seeds; the 13.6 bill check is A - B' <= 1 900."""
    rows = []
    for aj in a_jobs:
        if aj.key[1] == INF or aj.key[2] != 0.0:
            continue
        bj = [j for j in bp_jobs if j.key == aj.key]
        if not bj:
            continue
        ra, rb = C.recs(aj), C.recs(bj[0])
        n = min(len(ra), len(rb))
        if not n:
            continue
        d = [(ra[s]["share_any"] - rb[s]["share_any"]) * ra[s]["units_total"] for s in range(n)]
        sb = [j for j in b_jobs if j.key == aj.key]
        db = None
        if sb:
            rs = C.recs(sb[0])
            m = min(n, len(rs))
            if m:
                db = float(np.mean([(rb[s]["share_any"] - rs[s]["share_any"]) * ra[s]["units_total"] for s in range(m)]))
        rows.append((aj.key[1], float(np.mean(d)), db, rb))
    return rows


def gate_bill(cell, a_jobs, bp_jobs, b_jobs):
    return ["%s: A - B' = %.0f units at T=%s (> %.0f)" % (cell, d, B.TAG(t), BILL_STOP) for t, d, _, _ in bill_table(cell, a_jobs, bp_jobs, b_jobs) if d > BILL_STOP]


def window_onset_bind(r, a, b):
    """13.6 stop 3: onset_bind over the steady window [a, b], from the per-window counts of cap_series (n, n with y <= 1)."""
    n = k = 0
    for row in r["rev4"]["cap_series"]:
        if row[0] >= a - 1e-9 and row[0] + 0.5 <= b + 1e-9:
            n += row[1]
            k += row[7]
    return (k / n) if n else None


def m_onset(jobs):
    """Seed-mean onset_bind over the steady window of B' against A, per finite lifetime: {T: value or None}."""
    out = {}
    for bj in [j for j in jobs if is_bp(j) and j.key[1] != INF]:
        aj = [j for j in jobs if j.design == "A" and j.key == bj.key][0]
        vs = []
        for ra, rb in zip(C.recs(aj), C.recs(bj)):
            a, b, _ = CR.pair_window(ra, rb, None)
            v = window_onset_bind(rb, a, b)
            if v is not None:
                vs.append(v)
        out[bj.key[1]] = float(np.mean(vs)) if vs else None
    return out


def gate_onset(jobs):
    ob = m_onset(jobs)
    if ob and all(v is None or v >= 0.99 or v <= 0.01 for v in ob.values()):
        return ["MEASUREMENT DISCIPLINE 17, %s: onset_bind over the window is %s at every finite T (inert or saturated where the onset was predicted inside)" % (
            M, ", ".join("T=%s: %s" % (B.TAG(t), "-" if v is None else "%.3f" % v) for t, v in sorted(ob.items())))]
    return []


# ---- the readings -----------------------------------------------------------------------------------------------------------------------
def fmt(x, p=3):
    return "-" if x is None else ("%.*f" % (p, x))


def mean_of(rs, f):
    v = [f(r) for r in rs]
    v = [x for x in v if x is not None]
    return float(np.mean(v)) if v else None


def gauge_table(cell, jobs):
    """Per row: whole-run delivered, on time within tau, resends, and the B' gauges of 13.7 (seed means)."""
    L = ["| row | seeds | delivered | on time (whole run) | resends | ov_frac | n_cap_lt_c | onset_bind | onset_t | tov_resets | y_max | w_min_over_c | den_bind | floor_bind | deq_early_would |",
         "|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|"]
    for j in jobs:
        rs = C.recs(j)
        if not rs:
            continue
        r4 = lambda k: mean_of(rs, lambda r: r["rev4"].get(k))     # noqa: E731
        ot = mean_of(rs, lambda r: sum(r["series"]["ot"][CR.tkey(r["tau"])]) / max(sum(r["series"]["handed"]), 1))
        L.append("| %s | %d | %s | %s | %s | %s | %s | %s | %s | %s | %s | %s | %s | %s | %s |" % (
            C.cell_label(j), len(rs), fmt(mean_of(rs, lambda r: r["share_any"]), 4), fmt(ot, 4), fmt(mean_of(rs, lambda r: r["gauges"].get("repair_sends", 0)), 0),
            fmt(r4("ov_frac")), fmt(r4("n_cap_lt_c"), 0), fmt(r4("onset_bind")), fmt(r4("onset_t"), 2), fmt(r4("tov_resets"), 1), fmt(r4("y_max"), 2), fmt(r4("w_min_over_c")),
            fmt(r4("den_bind"), 0), fmt(r4("floor_bind"), 4), fmt(r4("deq_early_would"), 0)))
    return L


def dominance13(cell, builds, base, nf, double):
    """12.3's dominance reading for each build against each baseline, as confirm3e's, on the fixed reader.  Returns (lines, undetermined arms of B', best points)."""
    L, undet, best = [], set(), {}
    units = C.units_of(cell)
    L.append("| build | baseline | best point | share | on time | p50 | outcome |")
    L.append("|---|---|---|---|---|---|---|")
    for build, rows in builds:
        fin = [j for j in rows if j.key[1] != INF and j.key[2] == 0.0]
        if not fin:
            continue
        for bj in base:
            n = min([len(C.recs(bj))] + [len(C.recs(j)) for j in fin])
            if bj.name not in double and n > bj.nseeds:
                n = bj.nseeds
            nfv = C.nf_for(nf, cell, n)
            pts = []
            for j in fin:
                xs, ys = C.steady_arrays(cell, C.recs(j, n), C.recs(bj, n), n)
                pts.append((j, j.key[1], xs, CR.share_bar(float(ys["ot"].mean()), units, nfv["ot"]), ys))
            bp = CR.best_point([(p[0], p[1], p[2], p[3]) for p in pts])
            pt = [p for p in pts if p[0] is bp[0]][0]
            xs, ys = pt[2], pt[4]
            rds = {"share": CR.compare_pair(xs["share"], ys["share"], 1.0, CR.share_bar(float(ys["share"].mean()), units, nfv["share"])),
                   "ot": CR.compare_pair(xs["ot"], ys["ot"], 1.0, CR.share_bar(float(ys["ot"].mean()), units, nfv["ot"])),
                   "p50": CR.compare_pair(xs["p50"], ys["p50"], -1.0, CR.p50_bar(float(np.mean(np.where(np.isfinite(ys["p50"]), ys["p50"], 1e9))), nfv["p50"]))}
            video = cell in B.VIDEO
            word = CR.dominance_word({k: v for k, v in rds.items() if not (video and k == "p50")})
            if word == "undetermined" and build == "B'" and bj.name not in double:
                undet.add(bj.name)
                undet.add(pt[0].name)
            best[(build, bj.name)] = dict(T=pt[1], word=word, rds=rds, n=n, xs=xs, ys=ys, job=pt[0], bar=dict(share=CR.share_bar(float(ys["share"].mean()), units, nfv["share"]),
                                                                                                               p50=CR.p50_bar(float(np.mean(np.where(np.isfinite(ys["p50"]), ys["p50"], 1e9))), nfv["p50"])))
            L.append("| %s | %s | T=%s | %s | %s | %s | %s%s |" % (build, C.cell_label(bj), B.TAG(pt[1]), C.fmt(rds["share"]), C.fmt(rds["ot"]), C.fmt(rds["p50"]), word,
                                                              " (p50 reported only)" if video else ""))
    return L, undet, best


def pair_table(cell, bp_jobs, ref_jobs, label, nf):
    """B' against A (or B) at the same lifetime, on the paired steady window: reported only."""
    units = C.units_of(cell)
    L = ["| T | readout | B' - %s [95 %%] | bar |" % label, "|---|---|---|---|"]
    for bj in bp_jobs:
        if bj.key[1] == INF or bj.key[2] != 0.0:
            continue
        rj = [j for j in ref_jobs if j.key == bj.key]
        if not rj:
            continue
        n = min(len(C.recs(bj)), len(C.recs(rj[0])))
        if not n:
            continue
        nfv = C.nf_for(nf, cell, n)
        xs, ys = C.steady_arrays(cell, C.recs(bj, n), C.recs(rj[0], n), n)
        for k, sign in (("share", 1.0), ("ot", 1.0), ("p50", -1.0)):
            bar = CR.p50_bar(float(np.mean(np.where(np.isfinite(ys[k]), ys[k], 1e9))), nfv["p50"]) if k == "p50" else CR.share_bar(float(ys[k].mean()), units, nfv[k])
            rd = CR.compare_pair(xs[k], ys[k], sign, bar)
            L.append("| %s | steady %s | %s (positive favours B') | %.3g |" % (B.TAG(bj.key[1]), k, C.fmt(rd), bar))
    return L


def bill_lines(cell, a_jobs, bp_jobs, b_jobs):
    rows = bill_table(cell, a_jobs, bp_jobs, b_jobs)
    L = ["| T | A - B' (units) | B' - B (units) | predicted mu_hat (c - x_hat w_end)+ | observed / predicted |", "|---|---|---|---|---|"]
    for t, d, db, rb in rows:
        pred = None
        rs = [r for r in C.recs([j for j in bp_jobs if j.key == ("lt", t, 0.0)][0])]
        ps = []
        for r in rs:
            cs = r["rev4"].get("cap_series") or []
            if cs and r["rev4"].get("dhat_mean") and cs[-1][6] is not None and cs[-1][2] is not None:
                last = cs[-1]
                ps.append(r["rev4"]["dhat_mean"] * max(last[6] - last[4] * last[2], 0.0))
        if ps:
            pred = float(np.mean(ps))
        L.append("| %s | %.0f | %s | %s | %s |" % (B.TAG(t), d, "-" if db is None else "%.0f" % db, "-" if pred is None else "%.0f" % pred,
                                                  "-" if not pred else "%.2f" % (d / pred)))
    return L


def read_ov(cell, jobs, nf, double=frozenset()):
    """One overload cell: the gauge table, the dominance reading of A, B (stored) and B', and B' against A and B.  Returns (lines, undetermined arms, best points)."""
    A = [j for j in jobs if j.design == "A" and j.key[0] == "lt" and j.key[2] == 0.0]
    Bs = [j for j in jobs if j.design == "B" and not is_bp(j) and j.key[2] == 0.0]
    Bp = [j for j in jobs if is_bp(j) and j.key[2] == 0.0]
    base = [j for j in jobs if j.design in ("adr", "rt", "rteq", "bulk")]
    L = ["\n### %s (build B', 13.3)\n" % cell,
         "Builds: A = Revision 3 + clamp + floor repeat (evidenced_overdue) + elig rack; B = A + the old wait limit (stored, reported only); B' = A + the 13.3 wait limit (Bp), no send gate. "
         "Dominance on the paired steady window (12.2), best finite-T point per build, on the fixed reader (13.6). %s\n" % (
             "Delay p50 is reported only here (delta = 0.005, 12.3)." if cell in B.VIDEO else "")]
    L += gauge_table(cell, [j for j in jobs if j.design in ("A", "B", "adr", "rt", "rteq", "bulk") or is_bp(j)])
    L.append("\nDominance (12.3). The verdict is B'; A and B are reported only.\n")
    builds = [("A", A)] + ([("B", Bs)] if Bs else []) + [("B'", Bp)]
    dl, undet, best = dominance13(cell, builds, base, nf, double)
    L += dl
    L.append("\nB' against A, paired steady window (reported only):\n")
    L += pair_table(cell, Bp, A, "A", nf)
    if Bs:
        L.append("\nB' against B, paired steady window (reported only):\n")
        L += pair_table(cell, Bp, Bs, "B", nf)
    L.append("\nThe bill (whole-run delivered units; 13.1's fluid bill mu_hat (c - x_hat w_end)+ with w_end the last overload window's cap):\n")
    L += bill_lines(cell, A, Bp, Bs)
    return L, undet, best


# ---- OV-long-m's shape readouts (13.5 items 1-4) and the "worse" interpretation (13.8) ----------------------------------------------------
ONSET_BAND = {0.4: (5.3, 0.4), 0.6: (7.3, 0.5), 1.0: (11.3, 0.7), 2.0: (21.3, 1.2)}


def window_shares(recs):
    """Per 0.5 s window delivered share, one row per seed."""
    return np.asarray([[d / h if h else np.nan for d, h in zip(r["series"]["deliv"], r["series"]["handed"])] for r in recs])


def shape_readouts(T, a_recs, b_recs):
    """Items 1-4 of 13.5 for one lifetime, from the stored cap_series and the per-window shares.  Returns (lines, dict of pass flags, onset tov)."""
    n = min(len(a_recs), len(b_recs))
    a_recs, b_recs = a_recs[:n], b_recs[:n]
    L, ok = [], {}
    r4s = [r["rev4"] for r in b_recs]
    ot = [r["onset_t"] for r in r4s if r["onset_t"] is not None]
    sbs = [r["onset_tov"] for r in r4s if r["onset_tov"] is not None]
    t0s = [r["ov_t0"] for r in r4s if r["ov_t0"] is not None]
    if not ot:
        return ["T=%s: B' never reached the onset (onset_t None at every seed)" % B.TAG(T)], dict(item1=False, item2=False, item3=False, item4=False), None
    onset_t, sb, t0 = float(np.mean(ot)), float(np.mean(sbs)), float(np.mean(t0s))
    band = ONSET_BAND.get(T)
    ok["item1"] = (abs(onset_t - band[0]) <= band[1]) if band else None
    L.append("- item 1, onset: onset_t = %.2f s (seed mean of %d), t0 = %.2f s, s_b = onset_t - t0 = %.2f s; registered band %s: %s" % (
        onset_t, len(ot), t0, sb, "%.1f +- %.1f" % band if band else "-", {True: "inside", False: "OUTSIDE", None: "n/a"}[ok["item1"]]))
    a, b, _ = CR.pair_window(a_recs[0], b_recs[0], None)
    sh_a, sh_b = window_shares(a_recs), window_shares(b_recs)
    deficit = np.nanmean(sh_a - sh_b, axis=0)
    ser = {}
    for r in b_recs:
        for row in r["rev4"]["cap_series"]:
            ser.setdefault(row[0], []).append(row)
    pre = sorted(t for t in ser if a <= t and t + 0.5 <= onset_t)
    dev = max((abs(np.mean([x[2] * x[4] / x[6] for x in ser[t] if x[2] is not None and x[6]]) - 1.0) for t in pre if any(x[2] is not None and x[6] for x in ser[t])), default=None)
    d_pre = float(np.mean([deficit[int(t / 0.5)] for t in pre])) if pre else None
    ok["item2"] = (dev is not None and dev <= 0.03 and d_pre is not None and d_pre <= 0.003)
    L.append("- item 2, before the onset (%d windows from %.1f s): cap x_hat / c within %s of 1 (bar 3 %%); mean deficit against A %s (bar 0.003): %s" % (
        len(pre), a, "-" if dev is None else "%.1f %%" % (100 * dev), fmt(d_pre, 4), "holds" if ok["item2"] else "OUTSIDE"))
    lam = (a_recs[0]["units_total"] - 1) / a_recs[0]["last_hand"]
    mu = float(np.mean([r["dhat_mean"] for r in r4s if r["dhat_mean"]]))
    near = [x for t in ser if onset_t - 1.0 <= t <= onset_t for x in ser[t] if x[2] is not None and x[6]]
    cx = float(np.mean([x[6] / x[4] for x in near])) if near else float("nan")
    pred_peak = (mu / lam) * cx / (2.0 * sb)
    wins_after = [t for t in sorted(ser) if t >= onset_t - 0.5 and t + 0.5 <= b]
    sm = {t: float(np.mean([deficit[int(u / 0.5)] for u in (t - 0.5, t, t + 0.5) if 0 <= int(u / 0.5) < len(deficit)])) for t in wins_after}
    peak = max(sm.values()) if sm else float("nan")
    ok["item3"] = bool(sm) and abs(peak / pred_peak - 1.0) <= 0.30
    L.append("- item 3, after the onset: deficit peak (3-window mean) %.4f against (mu/lam)(c/x)/(2 s_b) = %.4f (mu %.0f, lam %.0f, c/x %.3f): ratio %.2f (bar +-30 %%): %s" % (
        peak, pred_peak, mu, lam, cx, peak / pred_peak if pred_peak else float("nan"), "holds" if ok["item3"] else "OUTSIDE"))
    fall = []
    for y in (2.0, 3.0, 5.0):
        t = t0 + y * sb
        if sm and wins_after[0] <= t <= wins_after[-1]:
            k = min(wins_after, key=lambda u: abs(u + 0.25 - t))
            fall.append("y=%g: observed %.4f, predicted %.4f" % (y, sm[k], pred_peak * 4.0 / (1.0 + y) ** 2))
    L.append("  the fall as (s_b/(t_ov+s_b))^2 (reported): %s" % ("; ".join(fall) if fall else "no window in range"))
    tail = []
    for t in sorted(ser):
        rows = [x for x in ser[t] if x[2] is not None]
        if rows and t >= a and t + 0.5 <= b and float(np.mean([x[5] for x in rows])) >= 2.0 * sb:
            tail.append(float(np.mean([x[2] for x in rows])) * float(np.mean([x[5] for x in rows])))
    if tail:
        m = float(np.mean(tail))
        ok["item4"] = max(abs(v / m - 1.0) for v in tail) <= 0.15
        L.append("- item 4, tail: w x t_ov over %d windows with t_ov >= 2 s_b: spread %.1f %% around its mean (bar 15 %%): %s" % (len(tail), 100 * max(abs(v / m - 1.0) for v in tail), "holds" if ok["item4"] else "OUTSIDE"))
    else:
        ok["item4"] = None
        L.append("- item 4, tail: no window with t_ov >= 2 s_b = %.1f s inside the steady window (n/a)" % (2 * sb))
    return L, ok, sb


def worse13(best, jobs, nf):
    """13.8 'worse in OV-long-m': the three conditions, printed beside the reading (B' at its best point against today's stored Realtime arm, delta = 0.1)."""
    L = ["\nThe 'worse' interpretation of 13.8 (printed whatever the word; it applies only when the word against the stored arm is 'worse'):\n"]
    rt = [j for j in jobs if j.design == "rt"]
    if not rt or ("B'", rt[0].name) not in best:
        return L + ["No stored-arm row."]
    bd = best[("B'", rt[0].name)]
    T = bd["T"]
    cell = M
    n = bd["n"]
    A = [j for j in jobs if j.design == "A" and j.key == ("lt", T, 0.0)][0]
    xs, ys = bd["xs"], bd["ys"]
    d_share = float(np.mean(xs["share"] - ys["share"]))
    d_p50 = float(np.mean(xs["p50"] - ys["p50"]))
    c1 = (-0.012 <= d_share <= -0.0096) and d_p50 <= 0.02
    ax, ay = C.steady_arrays(cell, C.recs(A, n), C.recs(rt[0], n), n)
    a_vs_rt = float(np.mean(ax["share"] - ay["share"]))
    bx, bb = C.steady_arrays(cell, C.recs(bd["job"], n), C.recs(A, n), n)
    b_vs_a = float(np.mean(bx["share"] - bb["share"]))
    brecs, arecs = C.recs(bd["job"], n), C.recs(A, n)
    a_, b_, _ = CR.pair_window(arecs[0], brecs[0], None)
    ser = {}
    for r in brecs:
        for row in r["rev4"]["cap_series"]:
            if row[2] is not None:
                ser.setdefault(row[0], []).append(row[2])
    ws = sorted(t for t in ser if a_ <= t and t + 0.5 <= b_)
    descent = None
    if ws:
        w_start, w_end = float(np.mean(ser[ws[0]])), float(np.mean(ser[ws[-1]]))
        lam = (arecs[0]["units_total"] - 1) / arecs[0]["last_hand"]
        mu = float(np.mean([r["rev4"]["dhat_mean"] for r in brecs if r["rev4"]["dhat_mean"]]))
        descent = -(mu / lam) * (w_start - w_end) / (b_ - a_)
    c2 = all(v is not None and abs(v - t) <= 0.002 for v, t in ((a_vs_rt, -0.0035), (b_vs_a, -0.0055), (descent, -0.004)))
    sh, ok, _ = shape_all(jobs)
    c3 = all(v for v in ok.values() if v is not None) if ok else False
    L.append("Best point T=%s, word against the stored arm: %s" % (B.TAG(T), bd["word"]))
    L.append("1. steady share B' - stored arm = %+.4f (registered window [-0.012, -0.0096]), p50 B' - stored arm = %+.3f s (<= +0.02): %s" % (d_share, d_p50, "holds" if c1 else "FAILS"))
    L.append("2. decomposition (each within +-0.002): A - stored arm = %+.4f (-0.0035); B' - A = %+.4f (-0.0055); in-window descent = %s (-0.004): %s" % (
        a_vs_rt, b_vs_a, "-" if descent is None else "%+.4f" % descent, "holds" if c2 else "FAILS"))
    L.append("3. shape readouts 1-4 at every lifetime: %s" % ("pass" if c3 else "do not all pass (see the shape readouts)"))
    L.append("Reading: %s" % (
        "all three hold: a 'worse' here is the 13.8 limit, not a law failure." if c1 and c2 and c3 else
        "NOT all three hold: a 'worse' here is a law finding (the law returns to derivation); an 'equivalent' does not need the interpretation." if bd["word"] == "worse" else
        "the word is not 'worse', so the interpretation is not invoked (the conditions above are reported)."))
    return L


def shape_all(jobs):
    """Shape readouts at every finite lifetime of OV-long-m; the ratio of onset_tov at T = 2 and T = 0.4 (13.5 item 1: 4.86 +- 10 %)."""
    L, flags, sbs = [], {}, {}
    for bj in [j for j in jobs if is_bp(j) and j.key[1] != INF]:
        aj = [j for j in jobs if j.design == "A" and j.key == bj.key][0]
        ls, ok, sb = shape_readouts(bj.key[1], C.recs(aj), C.recs(bj))
        L.append("T=%s:" % B.TAG(bj.key[1]))
        L += ls
        for k, v in ok.items():
            flags["%s@%s" % (k, B.TAG(bj.key[1]))] = v
        sbs[bj.key[1]] = sb
    if sbs.get(2.0) and sbs.get(0.4):
        ratio = sbs[2.0] / sbs[0.4]
        flags["ratio"] = abs(ratio / 4.86 - 1.0) <= 0.10
        L.append("item 1, linearity in (c + x_hat o_hat): s_b(T=2) / s_b(T=0.4) = %.2f against 4.86 +- 10 %%: %s" % (ratio, "holds" if flags["ratio"] else "OUTSIDE"))
    return L, flags, sbs


VERDICT_SENTENCE = (
    "The verdict sentence (13.6): at delta = 0.1 every continuous relaxation puts the new model's steady share at the bar against today's stored Realtime arm at the best point "
    "(0.008-0.010 behind it, against a bar of about 0.0096). The relaxation (Q33) decides only whether the delay gain lands inside the 40 s window. The registered default (R1) is "
    "predicted 'equivalent or worse'; a 'worse' is read as that limit only if the three conditions of 13.8 hold, otherwise it is a law finding.")


def read_m(jobs, nf, double=frozenset()):
    lines, undet, best = read_ov(M, jobs, nf, double)
    lines.append("\nShape readouts of OV-long-m (13.5 items 1-4), read before the dominance outcome; from cap_series and the per-window shares:\n")
    sl, flags, _ = shape_all(jobs)
    lines += sl
    lines.append("\n" + VERDICT_SENTENCE)
    lines += worse13(best, jobs, nf)
    return lines, undet, best


def read_ident(pairs, rows, xdiff, xr4):
    L = ["\n### Headroom identity (13.6 stop 1): B' against A, trace for trace\n",
         "| cell | T | seed | identical | n_cap_lt_c | tov_resets | y_max | w_min_over_c | ov_frac | onset_t | n_ticks |", "|---|---|---|---|---|---|---|---|---|---|---|"]
    for cell, T, s, d, r4 in rows:
        L.append("| %s | %s | %d | %s | %s | %s | %s | %s | %s | %s | %s |" % (cell, B.TAG(T), s, "yes" if not d else "NO: %s" % d, r4["n_cap_lt_c"], r4["tov_resets"], fmt(r4["y_max"], 4),
                                                                              fmt(r4["w_min_over_c"], 4), fmt(r4["ov_frac"], 4), fmt(r4["onset_t"], 3), r4["n_ticks"]))
    if xdiff is not None:
        L.append("| CL-long-v | 2 | %d (traced pair) | %s | %s | %s | %s | %s | %s | %s | %s |" % (SEED_X, "yes" if not xdiff else "NO: %s" % xdiff, xr4["n_cap_lt_c"], xr4["tov_resets"],
                                                                                              fmt(xr4["y_max"], 6), fmt(xr4["w_min_over_c"], 4), fmt(xr4["ov_frac"], 4), fmt(xr4["onset_t"], 3), xr4["n_ticks"]))
    return L


def read_kov(jobs, nf=None):
    L = ["\n### K-OV (reported only, 13.8; whole-run readouts, no steady window)\n",
         "Predicted: A 0.70-0.73 delivered, on time (0.4 s) at delivered - <= 0.01, p50 0.30-0.34 s; B' 0.70-0.73, on time at delivered - <= 0.02, p50 0.15-0.19 s. "
         "The old law's sizing row: 0.720 at p50 0.166 s.\n",
         "| build | seeds | delivered | on time within 0.4 s | p50 from hand-over (whole run) | resends | n_cap_lt_c | onset_t | y_max | wall s (mean) |", "|---|---|---|---|---|---|---|---|---|---|"]
    for j in jobs:
        rs = C.recs(j)
        if not rs:
            continue
        ro = [CR.readouts(r["series"], list(range(len(r["series"]["handed"]))), r["tau"]) for r in rs]
        L.append("| %s | %d | %s | %s | %s | %s | %s | %s | %s | %s |" % ("B'" if is_bp(j) else "A", len(rs), fmt(float(np.mean([r["share_any"] for r in rs])), 4), fmt(float(np.mean([o["ot"] for o in ro])), 4),
                                                                          fmt(float(np.mean([o["p50"] for o in ro])), 3), fmt(mean_of(rs, lambda r: r["gauges"].get("repair_sends", 0)), 0),
                                                                          fmt(mean_of(rs, lambda r: r["rev4"].get("n_cap_lt_c")), 0), fmt(mean_of(rs, lambda r: r["rev4"].get("onset_t")), 3),
                                                                          fmt(mean_of(rs, lambda r: r["rev4"].get("y_max")), 2), fmt(mean_of(rs, lambda r: r.get("wall_s")), 0)))
    return L


# ---- the A/A floor of the new cell ----------------------------------------------------------------------------------------------------------
def freeze_nf13(smoke):
    """The A/A floors the readings use: the stored cells' frozen ones (12.3), OV-long-m's from its own A/A rows.  A smoke run has none stored and computes all three."""
    nf = {}
    if os.path.exists(C.nf_path()) and not smoke:
        with open(C.nf_path()) as f:
            nf = json.load(f)
    aa = {}
    for j in C.aa_jobs() + aa_m():
        if j.cell in (OV_CELLS + (M,)) and (smoke or j.cell == M):
            aa.setdefault(j.cell, {})[j.key[1]] = j
    for cell, d in aa.items():
        ra, rb = C.recs(d[0]), C.recs(d[1])
        nf[cell] = {str(n): CR.nf_from_aa(ra, rb, C.dead_of(cell), n) for n in (3, 6, 12) if min(len(ra), len(rb)) >= n}
    p = os.path.join(C.OUT, "confirm13_nf.json")
    with open(p, "w") as f:
        json.dump(nf, f, indent=1)
    return nf


def append_results(lines):
    with open(RESULTS, "a", encoding="utf-8") as f:
        f.write("\n".join(lines) + "\n")


# ---- the chain --------------------------------------------------------------------------------------------------------------------------
def reading(fn, *args, **kw):
    """A reading that cannot be formed (an empty paired window).  A smoke run, whose short runs leave the video cell without a steady window, prints it and goes on; a real run raises, so
    no DONE13 marker is written and the stored runs are read again."""
    try:
        return fn(*args, **kw)
    except (ValueError, IndexError) as e:
        if not C.SMOKE:
            raise
        return ["### no reading (smoke): %s: %r" % (fn.__name__, e)], set(), {}


def double_arms(pool, jobs, undet, tag):
    """The registered doubling (12.3): the arms of the undetermined comparisons run twice their seeds (at most 24)."""
    dj = [j for j in jobs if j.name in undet]
    extra = []
    for j in dj:
        n2 = min(2 * j.nseeds, MAX_UNITS)
        extra += [(j.fp, s) for s in range(j.nseeds, n2)]
    print("[%s] %s: doubling %d arms (%d runs)" % (time.strftime("%H:%M:%S"), tag, len(dj), len(extra)), flush=True)
    C.run_pool(pool, extra, tag + " doubling")
    for j in dj:
        j.nseeds = min(2 * j.nseeds, MAX_UNITS)


def finish(part, lines):
    append_results(lines)
    C.marker("DONE13-" + part)


def run_chain13(workers, only=None):
    smoke = bool(C.SMOKE)
    os.makedirs(C.OUT, exist_ok=True)
    reg = dict(smoke=dict(C.SMOKE), out=C.OUT)
    for j in all_jobs13():
        C.JOBS_BY_FP[j.fp] = j
    C.PEAKS.update(C.load_peaks())
    if stopped13():
        print("STOP13 marker present: not starting", flush=True)
        return 1
    pj = part_jobs()
    pairs = ident_pairs()
    with mp.Pool(workers, initializer=init_worker13, initargs=(reg,)) as pool:
        def todo(part):
            return not (only and part != only) and not C.done("DONE13-" + part)

        def gate(part, bad):
            if bad and smoke:
                print("smoke: gate failures (a real run stops here): %s" % "; ".join(bad[:5]), flush=True)
                append_results(["### %s: gate failures (smoke, not stopped)" % part] + ["- " + b for b in bad[:20]])
                return True
            if bad:
                stop13("part %s: %s" % (part, "; ".join(bad[:10])))
                append_results(["\n### %s: STOPPED by a gate\n" % part] + ["- " + b for b in bad[:40]])
                return False
            return True

        if todo("AA-m"):
            print("[%s] A/A rows of OV-long-m" % time.strftime("%H:%M:%S"), flush=True)
            C.run_pool(pool, C.tasks_of(pj["AA-m"]), "AA-m")
            nf = freeze_nf13(smoke)
            finish("AA-m", ["\n## Build B' confirmation (three-knobs-revision4.md 13.6) (%s): the A/A floors\n" % time.strftime("%Y-%m-%d"), "```", json.dumps(nf.get(M, {}), indent=1), "```"])
        nf = freeze_nf13(smoke)
        if todo("IDENT"):
            print("[%s] identity set" % time.strftime("%H:%M:%S"), flush=True)
            C.run_pool(pool, C.tasks_of([a for a, _ in pairs] + pj["IDENT"]), "IDENT")
            bad = absolutes([b for _, b in pairs])
            b2, rows = gate_identity(pairs)
            xd, xr4 = seed_x_diff()
            if xd:
                b2.append("B' != A in CL-long-v T=2 seed %d (traced pair): %s" % (SEED_X, xd))
            if not gate("IDENT", bad + b2):
                return 1
            finish("IDENT", read_ident(pairs, rows, xd, xr4))
        for cell in OV_CELLS:
            if not todo(cell):
                continue
            old = C.jobs_of(cell)
            jobs = old + pj[cell]
            print("[%s] cell %s: %d new runs" % (time.strftime("%H:%M:%S"), cell, runs_of(pj[cell])), flush=True)
            C.run_pool(pool, C.tasks_of(old + pj[cell]), cell)
            a_j = [j for j in old if j.design == "A" and j.key[2] == 0.0]
            b_j = [j for j in old if j.design == "B"]
            bad = absolutes(pj[cell])
            sa = [j for j in pj[cell] if j.key == ("lt", INF, INF)]
            bu = [j for j in old if j.design == "bulk"]
            bad += gate_s5(sa, bu)
            if cell == "OV-long-v":
                bad += gate_bill(cell, a_j, [j for j in pj[cell] if j.key[2] == 0.0], b_j)
            if not gate(cell, bad):
                return 1
            lines, undet, _ = reading(read_ov, cell, jobs, nf)
            if undet:
                double_arms(pool, jobs, undet, cell)
                lines, _, _ = reading(read_ov, cell, jobs, nf, double=frozenset(undet))
            finish(cell, lines)
        if todo("OV-long-m"):
            jobs = m_jobs()
            print("[%s] cell %s: %d new runs" % (time.strftime("%H:%M:%S"), M, runs_of(pj[M])), flush=True)
            C.run_pool(pool, C.tasks_of(jobs), M)
            bad = absolutes([j for j in jobs if j.design in ("A", "B", "adr")]) + gate_onset(jobs)
            if not gate(M, bad):
                return 1
            lines, undet, _ = reading(read_m, jobs, nf)
            if undet:
                double_arms(pool, jobs, undet, M)
                lines, _, _ = reading(read_m, jobs, nf, double=frozenset(undet))
            finish("OV-long-m", lines)
        if todo("K-OV"):
            jobs = kov_jobs()
            C.run_pool(pool, C.tasks_of(jobs), "K-OV")
            if not gate("K-OV", absolutes(jobs)):
                return 1
            finish("K-OV", read_kov(jobs))
    if not only:
        C.marker("DONE13")
    return 0


def main(argv=None):
    ap = argparse.ArgumentParser()
    ap.add_argument("--workers", type=int, default=9)
    ap.add_argument("--only", default=None, choices=PARTS)
    ap.add_argument("--smoke", action="store_true", help="exercise the whole chain on a fraction of the units, 3 seeds, in a scratch out dir")
    ap.add_argument("--smoke-out", default=None, help="reuse a smoke directory")
    ap.add_argument("--smoke-scale", type=float, default=SMOKE_SCALE)
    ap.add_argument("--counts", action="store_true", help="print the planned run counts (13.6)")
    a = ap.parse_args(argv)
    B.build()
    if a.counts:
        reg, fresh, miss, todo = counts()
        for p, n in reg.items():
            print("%-10s %d" % (p, n))
        print("total registered new runs: %d (of which %d are ADR-0084 / Bulk rows at delta = 0.1 whose fingerprint differs from OV-long's: 13.6's 254 + 9 = 263)" % (sum(reg.values()), fresh))
        print("stored rows read and missing from the store: %d; new runs still to run: %d" % (miss, todo))
        return 0
    if a.smoke:
        import tempfile
        C.SMOKE.update(scale=a.smoke_scale, seeds=SMOKE_SEEDS)
        C.OUT = a.smoke_out or tempfile.mkdtemp(prefix="confirm13_smoke_")
        global RESULTS
        RESULTS = os.path.join(C.OUT, "results-smoke.md")
        print("smoke run in", C.OUT, flush=True)
    return run_chain13(min(a.workers, 9), a.only)


if __name__ == "__main__":
    mp.freeze_support()
    sys.exit(main())
