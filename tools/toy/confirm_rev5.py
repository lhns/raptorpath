"""Build B'' (three-knobs-revision4.md 14): the ramp R5 and the net repair clamp, against today's Realtime (both arms) and every other design.

    python confirm_rev5.py [--workers 4] [--only PART] [--counts]

It reuses confirm3e.py / confirm13.py / confirm13q.py's machinery (registry, store keyed by fingerprint, worker pool, readers); no stored fingerprint changes (the new fields are
left out of the hash at their defaults).  CONFIRM_OUT points at the store (a copy of the stage's store; never the original).  Parts, in order (each writes DONE5-<part>; a stop
writes STOP5 and ends the process):

    GAUGE      reads only: build A with net_gauge at the identity set (14.5), seeds 0-2; checked equal to the stored A rows; counts where the net clamp would act
    IDENT      B'' = A, trace for trace, in CLc, CLc-v, CL-long-v at their B_T (seeds 0-2) and CL-long-v T = 2 seed 11
    OV-long-v  B'' at the 8 rows, 6 seeds; B'' = B' on every key at every row and seed (14.5)
    OV-long    B'' at the 8 rows, 6 seeds; dominance
    OV-long-m  B'' at 5 lifetimes, 6 seeds; dominance against the stored rows
    OV-long-q  the same at delta = 0.118
    CL-long, CL, CLw-long   B'' over build A's whole sweep, 12 seeds, read as the new curve with the registered matched-share reader (one doubling)
    K-OV       B'' at T = 0.4, 6 seeds, reported only

Results go to results-rev5.md.
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
import batch3e as B  # noqa: E402
import confirm13 as K  # noqa: E402
import confirm13q as KQ  # noqa: E402
import confirm3e as C  # noqa: E402
import confirm_read as CR  # noqa: E402

INF = math.inf
RESULTS = os.environ.get("REV5_RESULTS", os.path.join(HERE, "results-rev5.md"))
BPP_KW = dict(K.A_KW, wait_limit=True, wait_limit_form="rev5", clamp_net=True)
GAUGE_KW = dict(K.A_KW, net_gauge=True)
FULL_CELLS = ("CL-long", "CL", "CLw-long")
IDENT_CELLS = ("CLc", "CLc-v", "CL-long-v")
OV_CELLS = ("OV-long-v", "OV-long")
MID = (K.M, KQ.Q)
LOW, LOW_DELTA = "OV-long-c", 0.05                   # 14.8: one row below the predicted boundary (registered before it ran)

if LOW not in B.CELLS:
    B.CELLS[LOW] = B._copy(B.CELLS, "OV-long")
    B.SWEEP[LOW] = B.SWEEP["OV-long"]
    _delta_of5 = B.delta_of
    B.delta_of = lambda cell: LOW_DELTA if cell == LOW else _delta_of5(cell)
PARTS = ("GAUGE", "IDENT", "OV-long-v", "OV-long", K.M, KQ.Q) + FULL_CELLS + ("K-OV", LOW)
MAX_UNITS = 24


# ---- the registry -------------------------------------------------------------------------------------------------------------------------
def bpp_job(cell, key, nseeds, trace_seeds=0, design="B"):
    return C.CJob("%s | Bpp | %s" % (cell, B.row_name(key)), cell, design, key, nseeds, C.mk(cell, key, BPP_KW), trace_seeds=trace_seeds)


def is_bpp(j):
    return " | Bpp | " in j.name


def gauge_jobs():
    out = []
    for cell in FULL_CELLS + IDENT_CELLS:
        for t in C.B_T.get(cell, C.B_T_DEFAULT):
            key = ("lt", t, 0.0)
            out.append(C.CJob("%s | A-gauge | %s" % (cell, B.row_name(key)), cell, "A", key, C.SEEDS_B, C.mk(cell, key, GAUGE_KW)))
    return out


def ident_pairs():
    """(stored A job, B'' job) at the B_T of the three cells with no outage drain, seeds 0-2 traced (the stored A rows there carry traces)."""
    out = []
    for cell in IDENT_CELLS:
        ts = C.B_T.get(cell, C.B_T_DEFAULT)
        a_by = {j.key[1]: j for j in C.jobs_of(cell) if j.design == "A" and j.key[0] == "lt" and j.key[2] == 0.0}
        for t in ts:
            key = ("lt", t, 0.0)
            out.append((a_by[t], bpp_job(cell, key, C.SEEDS_B, trace_seeds=C.SEEDS_B)))
    return out


def seed_x_jobs():
    a = [j for j in K.seed_x_pair() if " | A | " in j.name][0]
    cell, key = "CL-long-v", ("lt", 2.0, 0.0)
    base = C.mk(cell, key, BPP_KW)
    j = C.CJob("%s | Bpp | %s (traced, seed %d)" % (cell, B.row_name(key), K.SEED_X), cell, "B", key, 1, (lambda s, base=base: base(K.SEED_X)), trace_seeds=1)
    j.fp = j.fp + "t%d" % K.SEED_X
    return a, j


def ov_jobs(cell):
    ns = C.seeds_head(cell)
    return [bpp_job(cell, key, ns, trace_seeds=ns if key == ("lt", INF, INF) else 0) for key in C.sweep(cell)]


def mid_jobs(cell):
    keys = K.m_keys() if cell == K.M else KQ.q_keys()
    return [bpp_job(cell, key, 6) for key in keys]


def full_jobs(cell):
    """B'' over build A's whole sweep, as the new curve (design "A" for the reader), 12 seeds; the T = inf, H = inf row traced (the Bulk identity)."""
    ns = C.seeds_head(cell)
    return [bpp_job(cell, key, ns, trace_seeds=ns if key == ("lt", INF, INF) else 0, design="A") for key in C.sweep(cell)]


def kov_jobs():
    return [bpp_job("K-OV", ("lt", 0.4, 0.0), 6)]


def low_rows():
    """14.8: OV-long-c (delta 0.05): A at T = 0.4 (6 seeds), today's Realtime at delta 0.05, both arms (3 seeds)."""
    key, d = ("lt", 0.4, 0.0), ("rt", LOW_DELTA)
    return [C.CJob("%s | A | %s" % (LOW, B.row_name(key)), LOW, "A", key, 6, C.mk(LOW, key, K.A_KW)),
            C.CJob("%s | rt | %s" % (LOW, B.row_name(d)), LOW, "rt", d, C.SEEDS_END, C.mk(LOW, d, {}, rt_cwnd=True)),
            C.CJob("%s | rteq | %s (engine queue)" % (LOW, B.row_name(d)), LOW, "rteq", d, C.SEEDS_END, C.mk(LOW, d, dict(eq=True), rt_cwnd=True))]


def stored_of(cell):
    if cell == LOW:
        return low_rows()
    if cell == K.M:
        return K.m_jobs()
    if cell == KQ.Q:
        return KQ.q_jobs()
    if cell == "K-OV":
        return K.kov_jobs()
    return C.jobs_of(cell) + (K.ov_new(cell) if cell in OV_CELLS else [])


def part_jobs():
    return {"GAUGE": gauge_jobs(), "IDENT": [b for _, b in ident_pairs()] + [seed_x_jobs()[1]], "OV-long-v": ov_jobs("OV-long-v"), "OV-long": ov_jobs("OV-long"),
            K.M: mid_jobs(K.M), KQ.Q: mid_jobs(KQ.Q), "CL-long": full_jobs("CL-long"), "CL": full_jobs("CL"), "CLw-long": full_jobs("CLw-long"), "K-OV": kov_jobs(),
            LOW: [bpp_job(LOW, ("lt", 0.4, 0.0), 6)] + low_rows()}


def all_jobs5():
    out = [j for js in part_jobs().values() for j in js]
    for c in OV_CELLS + MID + FULL_CELLS + IDENT_CELLS + ("K-OV", LOW):
        out += stored_of(c)
    out.append(seed_x_jobs()[0])
    return out


def init_worker5(reg):
    C.init_worker(reg)
    for j in all_jobs5():
        C.JOBS_BY_FP[j.fp] = j


# ---- helpers ----------------------------------------------------------------------------------------------------------------------------
def fmt(x, p=4):
    return "-" if x is None else ("%.*f" % (p, x))


def mean_of(rs, f):
    v = [f(r) for r in rs]
    v = [x for x in v if x is not None]
    return float(np.mean(v)) if v else None


def stop5(text):
    with open(os.path.join(C.OUT, "STOP5"), "w") as f:
        f.write(text + "\n")
    print("STOP: " + text, flush=True)


def append(lines):
    with open(RESULTS, "a", encoding="utf-8") as f:
        f.write("\n".join(lines) + "\n")


def load_nf():
    nf = {}
    for name in ("confirm_nf.json", "confirm13_nf.json", "confirm13q_nf.json"):
        p = os.path.join(C.OUT, name)
        with open(p) as f:
            for k, v in json.load(f).items():
                nf.setdefault(k, v)
    nf.setdefault(LOW, nf[KQ.Q])                     # 14.8: OV-long-q's own A/A floor (the same link, load and T; registered)
    return nf


def net_of(r):
    return r["rev4"].get("net") or {}


def absolutes(jobs):
    return K.absolutes(jobs)


# ---- readings ---------------------------------------------------------------------------------------------------------------------------
def read_gauge(jobs):
    """Each gauge run equals the stored A row (id3e keys) and reports where the net clamp would act."""
    stored = {}
    for cell in FULL_CELLS + IDENT_CELLS:
        for j in C.jobs_of(cell):
            if j.design == "A":
                stored[(cell, j.key)] = j
    L = ["\n## GAUGE (14.5, reads only): build A with the net clamp's gauge, against the stored A rows\n",
         "| cell | T | seed | equal to stored A | first sends gauged | net_bind | repair credits removed | stored repairs |", "|---|---|---|---|---|---|---|---|"]
    bad = []
    for j in jobs:
        for s in range(j.nseeds):
            r, a = C.load(j.fp, s), C.load(stored[(j.cell, j.key)].fp, s)
            d = C.ident_diff(a, r) if a else ["no stored row"]
            d = [x for x in d if x != "trace"]                                           # the gauge rows are not traced
            if d:
                bad.append("%s seed %d: %s" % (j.name, s, d))
            n = net_of(r)
            L.append("| %s | %s | %d | %s | %s | %s | %s | %s |" % (j.cell, B.TAG(j.key[1]), s, "yes" if not d else "NO %s" % d, n.get("n"), n.get("bind"), fmt(n.get("removed"), 1), r["gauges"]["rep_sent"]))
    return L, bad


def read_ident(pairs, xa, xb):
    L = ["\n## IDENT (14.5): B'' against A, trace for trace\n", "| cell | T | seed | identical | net_bind | n_cap_lt_c | y_max |", "|---|---|---|---|---|---|---|"]
    bad = []
    for a, b in pairs + [(xa, xb)]:
        for s in range(b.nseeds):
            ra, rb = C.load(a.fp, s), C.load(b.fp, s)
            d = C.ident_diff(ra, rb)
            if d:
                bad.append("%s seed %d: %s" % (b.name, s, d))
            L.append("| %s | %s | %s | %s | %s | %s | %s |" % (b.cell, B.TAG(b.key[1]), K.SEED_X if b is xb else s, "yes" if not d else "NO: %s" % d, net_of(rb).get("bind"),
                                                          rb["rev4"].get("n_cap_lt_c"), fmt(rb["rev4"].get("y_max"), 4)))
    return L, bad


def gauge_rows(cell, jobs):
    L = ["| row | seeds | whole-run delivered | on time (whole run) | repairs sent | decoded | resends | net_bind (mean) | repair credits removed (mean) | onset_t | y_max | w_min_over_c |",
         "|---|---|---|---|---|---|---|---|---|---|---|---|"]
    for j in jobs:
        rs = C.recs(j)
        if not rs:
            continue
        ot = mean_of(rs, lambda r: sum(r["series"]["ot"][CR.tkey(r["tau"])]) / max(sum(r["series"]["handed"]), 1))
        L.append("| %s | %d | %s | %s | %s | %s | %s | %s | %s | %s | %s | %s |" % (
            C.cell_label(j), len(rs), fmt(mean_of(rs, lambda r: r["share_any"])), fmt(ot), fmt(mean_of(rs, lambda r: r["gauges"]["rep_sent"]), 0), fmt(mean_of(rs, lambda r: r["gauges"]["dec_units"]), 0),
            fmt(mean_of(rs, lambda r: r["gauges"].get("repair_sends", 0)), 0), fmt(mean_of(rs, lambda r: net_of(r).get("bind")), 0), fmt(mean_of(rs, lambda r: net_of(r).get("removed")), 1),
            fmt(mean_of(rs, lambda r: r["rev4"].get("onset_t")), 2), fmt(mean_of(rs, lambda r: r["rev4"].get("y_max")), 2), fmt(mean_of(rs, lambda r: r["rev4"].get("w_min_over_c")), 4)))
    return L


def tail_check(jobs):
    """R5's tail (14.3): per cap_series window with y > 1.05, w t_ov against (c / xb) s_b from the window's own inputs (s_b = t_ov / y, y from the run's o_hat at the onset)."""
    L = ["| T | windows | median abs(w t_ov / ((c/xb) s_b) - 1) | max |", "|---|---|---|---|"]
    for j in jobs:
        if j.key[1] == INF:
            continue
        errs = []
        for r in C.recs(j):
            sb = r["rev4"].get("onset_tov")
            for t, n, w, age, x, tov, cp, n_on in r["rev4"].get("cap_series") or []:
                if w is None or cp is None or not sb or tov < 1.05 * sb or n_on:
                    continue
                cx = cp / max(1.0, x)
                errs.append(abs(w * tov / (cx * sb) - 1.0))
        if errs:
            L.append("| %s | %d | %.4f | %.4f |" % (B.TAG(j.key[1]), len(errs), float(np.median(errs)), float(np.max(errs))))
    return L


def read_dom(cell, new, nf, double=frozenset()):
    """Dominance of A, B' (stored) and B'' against each baseline, on the fixed reader; B'' - B' and B'' - A paired.  Returns (lines, undetermined arms, best points of B'')."""
    st = stored_of(cell)
    A = [j for j in st if j.design == "A" and j.key[0] == "lt" and j.key[2] == 0.0]
    Bp = [j for j in st if K.is_bp(j) and j.key[2] == 0.0]
    base = [j for j in st if j.design in ("adr", "rt", "rteq", "bulk")]
    Bpp = [j for j in new if j.key[2] == 0.0]
    L = ["\n## %s (build B'', 14)\n" % cell, "B'' = A + R5 (wait_limit_form rev5) + the net repair clamp (clamp_net). A and B' are the stored rows. Dominance on the paired steady window (12.2), "
         "best finite-T point per build, on the fixed reader (13.6). %s\n" % ("Delay p50 is reported only here (delta = 0.005, 12.3)." if cell in B.VIDEO else "")]
    L += gauge_rows(cell, A + Bp + new + base)
    dl, _, best = K.dominance13(cell, [("A", A), ("B'", Bp), ("B''", Bpp)], base, nf, double)
    undet = set()
    for (build, bname), v in best.items():
        if build == "B''" and v["word"] == "undetermined" and bname not in double:
            undet.add(bname)
            undet.add(v["job"].name)
    L.append("\nDominance (12.3):\n")
    L += dl
    L.append("\nB'' against B' and against A at each lifetime, paired steady window (reported only; 'positive favours B''' reads 'positive favours the first'):\n")
    L += [x.replace("B' -", "B'' -").replace("favours B'", "favours B''") for x in K.pair_table(cell, Bpp, Bp, "B'", nf)]
    L += [x.replace("B' -", "B'' -").replace("favours B'", "favours B''") for x in K.pair_table(cell, Bpp, A, "A", nf)]
    L.append("\nWhole-run delivered units, A - B'' and B' - B'' (seed means over the common seeds):\n")
    L.append("| T | A - B'' | B' - B'' |")
    L.append("|---|---|---|")
    for j in Bpp:
        aa = [x for x in A if x.key == j.key]
        bb = [x for x in Bp if x.key == j.key]
        rn = C.recs(j)
        n = len(rn)
        dA = float(np.mean([C.recs(aa[0], n)[s]["delivered"] - rn[s]["delivered"] for s in range(min(n, len(C.recs(aa[0], n))))])) if aa else None
        dB = float(np.mean([C.recs(bb[0], n)[s]["delivered"] - rn[s]["delivered"] for s in range(min(n, len(C.recs(bb[0], n))))])) if bb else None
        L.append("| %s | %s | %s |" % (B.TAG(j.key[1]), fmt(dA, 0), fmt(dB, 0)))
    if cell in MID or cell == LOW:
        L.append("\nR5's tail (14.3): w t_ov = (c / xb) s_b per window past the onset:\n")
        L += tail_check(Bpp)
    return L, undet, {k[1]: v for k, v in best.items() if k[0] == "B''"}


def read_ovv_identity(new):
    """OV-long-v: B'' = B' on every key at every row and seed (14.5: r = 0 and y <= 1 throughout)."""
    st = {j.key: j for j in K.ov_new("OV-long-v")}
    bad, n = [], 0
    for j in new:
        for s in range(j.nseeds):
            d = C.ident_diff(C.load(st[j.key].fp, s), C.load(j.fp, s))
            d = [x for x in d if x != "trace"]
            n += 1
            if d:
                bad.append("%s seed %d: %s" % (j.name, s, d))
    return ["\nOV-long-v identity B'' = B' (every id3e key): %d runs, %s\n" % (n, "all identical" if not bad else "DIFFER: %s" % bad)], bad


def read_full(cell, new, nf, double=frozenset()):
    """The registered matched-share reading with B'' as the new curve, against every baseline of the cell; then B'' against A per row (reported)."""
    st = C.jobs_of(cell)
    base = [j for j in st if j.design in ("adr", "rt", "rteq", "bulk")]
    lines, undet = C.read_cell(cell, new + base, nf, double)
    lines = [x.replace("Builds: A = Revision 3", "Builds: the new curve here is B'' (A + R5 + the net repair clamp, 14); A = Revision 3").replace("Positive favours build A", "Positive favours B''")
             .replace("build A's H = 0 sweep", "B''s H = 0 sweep") for x in lines]
    lines.insert(1, "\n(B'' rows are labelled 'Bpp'.)\n")
    a_lines, _ = C.read_cell(cell, st, nf, double)
    words = lambda ls: {tuple(c.strip() for c in x.split("|")[1:3]): x.split("|")[5].strip() for x in ls if x.startswith("| ") and x.count("|") == 6 and not x.startswith("| baseline") and not x.startswith("| row")}   # noqa: E731
    wa, wb = words(a_lines), words(lines)
    lines.append("\nWords against every baseline: build A (stored rows, re-read now with the same reader and seeds) and B'' (14.5: every word unchanged is predicted):\n")
    lines.append("| baseline | readout | A | B'' | changed |")
    lines.append("|---|---|---|---|---|")
    for k in wb:
        lines.append("| %s | %s | %s | %s | %s |" % (k[0], k[1], wa.get(k, "-"), wb[k], "" if wa.get(k) == wb[k] else "**changed**"))
    A = {j.key: j for j in st if j.design == "A"}
    lines.append("\nB'' against the stored A at each row (id3e keys, seeds 0-%d); net clamp gauges:\n" % (C.seeds_head(cell) - 1))
    lines.append("| row | seeds identical | whole-run delivered A / B'' | whole-run on time A / B'' | recovery A / B'' | net_bind (mean) | credits removed (mean) | repairs A / B'' |")
    lines.append("|---|---|---|---|---|---|---|---|")
    for j in new:
        a = A[j.key]
        ra, rb = C.recs(a), C.recs(j)
        n = min(len(ra), len(rb))
        same = sum(1 for s in range(n) if not [x for x in C.ident_diff(ra[s], rb[s]) if x != "trace" or (j.trace_seeds and s < j.trace_seeds)])
        w = lambda rs, i: float(np.mean([CR.whole(r["series"], r["tau"])[i] for r in rs[:n]]))      # noqa: E731
        rec = lambda rs: float(np.mean([C.rec_of(r) for r in rs[:n]]))                               # noqa: E731
        lines.append("| %s | %d / %d | %.4f / %.4f | %.4f / %.4f | %.2f / %.2f | %s | %s | %.0f / %.0f |" % (
            C.cell_label(j), same, n, w(ra, 0), w(rb, 0), w(ra, 1), w(rb, 1), rec(ra), rec(rb), fmt(mean_of(rb, lambda r: net_of(r).get("bind")), 0),
            fmt(mean_of(rb, lambda r: net_of(r).get("removed")), 1), mean_of(ra, lambda r: r["gauges"]["rep_sent"]), mean_of(rb, lambda r: r["gauges"]["rep_sent"])))
    return lines, undet


def read_kov(jobs):
    L = ["\n## K-OV (reported only, 14.5; whole-run readouts)\n",
         "| build | seeds | delivered | on time within 0.4 s | p50 (whole run) | repairs sent | decoded | net_bind |", "|---|---|---|---|---|---|---|---|"]
    for j in K.kov_jobs() + jobs:
        rs = C.recs(j)
        ro = [CR.readouts(r["series"], list(range(len(r["series"]["handed"]))), r["tau"]) for r in rs]
        lab = "B''" if is_bpp(j) else ("B'" if K.is_bp(j) else "A")
        L.append("| %s | %d | %.4f | %.4f | %.3f | %.1f | %.1f | %s |" % (lab, len(rs), np.mean([r["share_any"] for r in rs]), np.mean([o["ot"] for o in ro]), np.mean([o["p50"] for o in ro]),
                                                                   np.mean([r["gauges"]["rep_sent"] for r in rs]), np.mean([r["gauges"]["dec_units"] for r in rs]), fmt(mean_of(rs, lambda r: net_of(r).get("bind")), 0)))
    return L


# ---- the chain --------------------------------------------------------------------------------------------------------------------------
def double_arms(pool, jobs, undet, tag):
    dj = [j for j in jobs if j.name in undet]
    extra = []
    for j in dj:
        extra += [(j.fp, s) for s in range(j.nseeds, min(2 * j.nseeds, MAX_UNITS))]
    print("[%s] %s: doubling %d arms (%d runs)" % (time.strftime("%H:%M:%S"), tag, len(dj), len(extra)), flush=True)
    C.run_pool(pool, extra, tag + " doubling")
    for j in dj:
        j.nseeds = min(2 * j.nseeds, MAX_UNITS)


def run_chain(workers, only=None):
    reg = dict(smoke={}, out=C.OUT)
    for j in all_jobs5():
        C.JOBS_BY_FP[j.fp] = j
    if os.path.exists(os.path.join(C.OUT, "STOP5")):
        print("STOP5 present", flush=True)
        return 1
    nf = load_nf()
    pj = part_jobs()
    ctx = mp.get_context("spawn")
    with ctx.Pool(workers, initializer=init_worker5, initargs=(reg,)) as pool:
        for part in PARTS:
            if only and part != only:
                continue
            if C.done("DONE5-" + part):
                continue
            jobs = pj[part]
            t0 = time.time()
            C.run_pool(pool, C.tasks_of(jobs), part)
            bad = absolutes(jobs)
            if part == "GAUGE":
                lines, b2 = read_gauge(jobs)
                bad += b2
            elif part == "IDENT":
                xa, xb = seed_x_jobs()
                C.run_pool(pool, C.tasks_of([xa]), part + " (A traced, seed 11)")
                lines, b2 = read_ident(ident_pairs(), xa, xb)
                bad += b2
            elif part in OV_CELLS or part in MID or part == LOW:
                lines, undet, best = read_dom(part, [j for j in jobs if is_bpp(j)], nf)
                if part == "OV-long-v":
                    il, b2 = read_ovv_identity(jobs)
                    lines += il
                    bad += b2
                if undet:
                    st = stored_of(part)
                    double_arms(pool, st + jobs, undet, part)
                    lines2, _, _ = read_dom(part, [j for j in jobs if is_bpp(j)], nf, double=frozenset(undet))
                    lines += ["\n### %s after the registered doubling (arms: %s)\n" % (part, sorted(undet))] + lines2
            elif part in FULL_CELLS:
                lines, undet = read_full(part, jobs, nf)
                if undet:
                    double_arms(pool, C.jobs_of(part) + jobs, undet, part)
                    lines2, _ = read_full(part, jobs, nf, double=frozenset(undet))
                    lines += ["\n### %s after the registered doubling (arms: %s)\n" % (part, sorted(undet))] + lines2
            else:
                lines = read_kov(jobs)
            lines.append("\n(%s: %d new runs, %.0f s wall at %d workers)\n" % (part, sum(j.nseeds for j in jobs), time.time() - t0, workers))
            append(lines)
            if bad:
                stop5("%s: %s" % (part, bad[:10]))
                return 1
            C.marker("DONE5-" + part)
            print("[%s] %s done" % (time.strftime("%H:%M:%S"), part), flush=True)
    return 0


def main(argv=None):
    ap = argparse.ArgumentParser()
    ap.add_argument("--workers", type=int, default=4)
    ap.add_argument("--only", default=None, choices=PARTS)
    ap.add_argument("--counts", action="store_true")
    a = ap.parse_args(argv)
    if a.counts:
        pj = part_jobs()
        for p in PARTS:
            js = pj[p]
            print("%-10s %4d runs (%d to do)" % (p, sum(j.nseeds for j in js), sum(1 for j in js for s in range(j.nseeds) if not C.have(j.fp, s))))
        st = [j for c in OV_CELLS + MID + FULL_CELLS + IDENT_CELLS + ("K-OV",) for j in stored_of(c)]
        print("stored rows missing:", sum(1 for j in st for s in range(j.nseeds) if not C.have(j.fp, s)))
        return 0
    return run_chain(a.workers, a.only)


if __name__ == "__main__":
    sys.exit(main())
