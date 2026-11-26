"""OV-long-q and the per-resend outcome gauge (three-knobs-revision4.md 13.9.5, 13.9.6): the middle-price row of build B', read with the corrected tests.

    python confirm13q.py [--workers 9] [--only PART] [--smoke] [--counts]

It reuses confirm13.py's machinery the way confirm13.py reuses confirm3e.py: the registry, the store (out3e/confirm/runs, keyed by fingerprint), the worker pool, the gates and the
dominance reader.  No stored fingerprint changes: the gauge field (resend_gauge) is off by default and left out of the hash there.  Parts, in order (each writes DONE13Q-<part>;
a failed gate writes STOP13Q and ends the process):

    AA-q          the A/A floor of OV-long-q: A at T = 0.4 against its phase variant, 2 x 12 seeds
    OV-long-q     OV-long's link, load, units and tau = 0.4 s at delta = 0.118: A and B' at T in {0.4, 0.6, 1, 2, inf} x 6 seeds (the gauge on, for the per-window resend
                  counts item 2 and 3 read), today's Realtime at delta = 0.118 (both arms, 3 seeds) and ADR-0084 / Bulk (3 seeds, reused by fingerprint when equal)
    RESEND-GAUGE  reported only: the stored A and B' rows of OV-long-v and OV-long-m at every finite T (H = 0), 3 seeds, rerun with the gauge on; checked equal to the stored rows

Results go to results-confirm13q.md.  --workers is capped at 20.
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
import confirm13 as K  # noqa: E402
import confirm3e as C  # noqa: E402
import confirm_read as CR  # noqa: E402
import toy2  # noqa: E402

INF = math.inf
RESULTS = os.environ.get("CONFIRM13Q_RESULTS", os.path.join(HERE, "results-confirm13q.md"))
Q, Q_DELTA = "OV-long-q", 0.118
PARTS = ("AA-q", "OV-long-q", "RESEND-GAUGE")
MAX_WORKERS = 20
GAUGE_SEEDS = 3
KAPPA_R = 0.065                                      # 13.9.5: units per extra resend that fill nothing (measured, not a law term)
REG = dict(x=1.235, o=0.095, R=0.104, t0=1.28)       # 13.9.6: the registered inputs of the model (x_hat, o_hat, R, t0)


# ---- the new cell, registered at module level so that spawned workers see it too ----------------------------------------------------------------
if Q not in B.CELLS:
    B.CELLS[Q] = B._copy(B.CELLS, "OV-long")           # OV-long's link, load, units and tau; only delta differs (13.9.6)
    B.SWEEP[Q] = B.SWEEP["OV-long"]
    _delta_of = B.delta_of
    B.delta_of = lambda cell: Q_DELTA if cell == Q else _delta_of(cell)

# 13.9.6, the registered predictions of B' (written before any run): s_b, onset_t, pre-onset windows, item 2 deficit, item 3 statistic, tail windows, steady share,
# on time (0.4 s), p50 on the A pairing, bill A - B' (units).  p50 band is 0.015 s below T = 1, 0.03 s at T = 1, 0.10 s at T = 2.
PRED = {
    0.4: dict(s_b=3.50, onset=4.78, pre=3, d2=0.0, item3=0.021, tail=62, share=0.799, ot="= share", p50=0.184, bill=3070, p50_band=0.015),
    0.6: dict(s_b=5.20, onset=6.48, pre=4, d2=0.0, item3=0.028, tail=55, share=0.797, ot="0.67", p50=0.292, bill=4340, p50_band=0.015),
    1.0: dict(s_b=8.59, onset=9.87, pre=7, d2=0.0018, item3=0.032, tail=42, share=0.792, ot="0.14", p50=0.578, bill=6190, p50_band=0.03),
    2.0: dict(s_b=17.1, onset=18.3, pre=14, d2=0.0018, item3=0.035, tail=8, share=0.787, ot="~ 0", p50=1.51, bill=7740, p50_band=0.10),
}
BANDS = dict(share=0.002, s_b=0.08, item3=0.15, item4=0.03, bill=250.0, ratio=0.10, cap=0.03)
RATIO_PRED = 4.87                                    # item 1: s_b(T = 2) / s_b(T = 0.4)


# ---- the registry ------------------------------------------------------------------------------------------------------------------------
def with_gauge(builder):
    return lambda s: dataclasses.replace(builder(s), resend_gauge=True)


def gauge_job(name, cell, design, key, nseeds, builder):
    """A row whose runs carry the resend gauge: a separate fingerprint (the field is not at its default), the same simulation."""
    return C.CJob(name, cell, design, key, nseeds, with_gauge(builder))


def q_keys():
    return [k for k in C.sweep(Q) if k[2] == 0.0]


def q_jobs():
    """OV-long-q: A and B' at the five lifetimes (H = 0, gauge on), today's Realtime at delta = 0.118 (both arms), ADR-0084 at its dial ends and Bulk."""
    out = []
    for key in q_keys():
        out.append(gauge_job("%s | A | %s" % (Q, B.row_name(key)), Q, "A", key, 6, C.mk(Q, key, K.A_KW)))
    for key in q_keys():
        out.append(gauge_job("%s | Bp | %s" % (Q, B.row_name(key)), Q, "B", key, 6, C.mk(Q, key, K.BP_KW)))
    d = ("rt", Q_DELTA)
    out.append(C.CJob("%s | rt | %s" % (Q, B.row_name(d)), Q, "rt", d, C.SEEDS_END, C.mk(Q, d, {}, rt_cwnd=True)))
    out.append(C.CJob("%s | rteq | %s (engine queue)" % (Q, B.row_name(d)), Q, "rteq", d, C.SEEDS_END, C.mk(Q, d, dict(eq=True), rt_cwnd=True)))
    for r in B.OLD_ENDS["OV-long"]["adr"]:
        out.append(C.CJob("%s | adr | %s" % (Q, B.row_name(("adr", r))), Q, "adr", ("adr", r), C.SEEDS_END, C.mk(Q, ("adr", r), {})))
    out.append(C.CJob("%s | bulk | %s" % (Q, B.row_name(("bulk",))), Q, "bulk", ("bulk",), C.SEEDS_END, C.mk(Q, ("bulk",), {})))
    return out


def aa_q():
    """A against its phase variant at T = 0.4, 12 seeds (the cell's own floor, gauge on)."""
    out = []
    T = B.primary_T(Q)
    for var in (0, 1):
        out.append(C.CJob("%s | AA%s | build A at T=%s" % (Q, "'" if var else "", B.TAG(T)), Q, "aa", ("aa", var), C.SEEDS_HEAD,
                          with_gauge(lambda s, var=var: C.smoke_fix(B.new_cfg(B.base_of(Q), s, T, 0.0, Q_DELTA, B.rp("RL", "worth"), variant=var, **dict(C.common(), **K.A_KW))))))
    return out


def gauge_rows():
    """The stored configurations of OV-long-v and OV-long-m at every finite T with H = 0: (cell, stored A job, stored B' job)."""
    out = []
    for cell in ("OV-long-v", K.M):
        if cell == K.M:
            a_jobs = [j for j in K.m_jobs() if j.design == "A"]
            bp_jobs = [j for j in K.m_jobs() if K.is_bp(j)]
        else:
            a_jobs = [j for j in C.jobs_of(cell) if j.design == "A" and j.key[0] == "lt" and j.key[2] == 0.0]
            bp_jobs = [K.bp_job(cell, j.key, C.seeds_head(cell)) for j in a_jobs]
        for aj in a_jobs:
            if aj.key[1] == INF:
                continue
            bj = [j for j in bp_jobs if j.key == aj.key][0]
            out.append((cell, aj, bj))
    return out


def gauge_jobs():
    """Reruns of the stored A and B' rows with the gauge on, 3 seeds (seeds 0-2 of the stored rows)."""
    out = []
    for cell, aj, bj in gauge_rows():
        for j in (aj, bj):
            out.append(C.CJob(j.name + " | gauge", cell, j.design, j.key, GAUGE_SEEDS, with_gauge(j.builder)))
    return out


def stored_pairs():
    """(stored job, its gauge rerun) for the identity check of the rerun against the stored run."""
    gj = gauge_jobs()
    out = []
    for cell, aj, bj in gauge_rows():
        for j in (aj, bj):
            g = [x for x in gj if x.name == j.name + " | gauge"][0]
            out.append((j, g))
    return out


def part_jobs():
    stored = K.m_stored_fps()
    return {
        "AA-q": aa_q(),
        "OV-long-q": [j for j in q_jobs() if not (j.design in ("adr", "bulk") and j.fp in stored)],
        "RESEND-GAUGE": gauge_jobs(),
    }


def all_jobs13q():
    out = [j for js in part_jobs().values() for j in js] + q_jobs() + [s for s, _ in stored_pairs()]
    return out


def init_worker13q(reg):
    K.init_worker13(reg)
    for j in all_jobs13q():
        C.JOBS_BY_FP[j.fp] = j


def counts():
    pj = part_jobs()
    reg = {p: K.runs_of(js) for p, js in pj.items()}
    fresh_old = sum(j.nseeds for j in pj["OV-long-q"] if j.design in ("adr", "bulk"))
    stored = [s for s, _ in stored_pairs()]
    miss = sum(1 for j in stored for s in range(j.nseeds) if not C.have(j.fp, s))
    todo = sum(1 for js in pj.values() for j in js for s in range(j.nseeds) if not C.have(j.fp, s))
    return reg, fresh_old, miss, todo


# ---- gates and markers -----------------------------------------------------------------------------------------------------------------
def stop13q(text):
    os.makedirs(C.OUT, exist_ok=True)
    with open(os.path.join(C.OUT, "STOP13Q"), "w") as f:
        f.write(text + "\n")
    print("STOP: " + text, flush=True)


def stopped13q():
    return os.path.exists(os.path.join(C.OUT, "STOP13Q"))


def append_results(lines):
    with open(RESULTS, "a", encoding="utf-8") as f:
        f.write("\n".join(lines) + "\n")


def finish(part, lines):
    append_results(lines)
    C.marker("DONE13Q-" + part)


def freeze_nf13q(smoke):
    """The cell's A/A floors (per seed count 3, 6, 12) on the paired window, from its own A/A rows."""
    aa = aa_q()
    ra, rb = C.recs(aa[0]), C.recs(aa[1])
    nf = {Q: {str(n): CR.nf_from_aa(ra, rb, C.dead_of(Q), n) for n in (3, 6, 12) if min(len(ra), len(rb)) >= n}}
    with open(os.path.join(C.OUT, "confirm13q_nf.json"), "w") as f:
        json.dump(nf, f, indent=1)
    return nf


# ---- the model of 13.9 (the reader's own, so that every prediction is printed beside its measurement) -------------------------------------------
def w_of(T, delta, t_ov, x=REG["x"], o=REG["o"], R=REG["R"]):
    """The law, as shipped: toy2.wait_limit_w."""
    return toy2.wait_limit_w(T, R, o, delta, x, t_ov)


def s_b_of(T, delta, x=REG["x"], o=REG["o"], R=REG["R"]):
    """The onset's overload time: y = 1, s_b = (c + x_bar o_hat) / delta."""
    return (max(T - R, 0.0) + max(x, 1.0) * o) / delta


def model_p50(T, delta, a, b, share, t0=REG["t0"], x=REG["x"], o=REG["o"], R=REG["R"]):
    """13.9.4, the reader's convention: the median over handed-over units with undelivered ones at +inf is the (0.5 / share)-quantile of the delivered delays, a delivered
    unit's delay being w(t_ov) + o_hat at its hand-over time, over the window [a, b] in 0.5 s steps."""
    if share <= 0.5:
        return math.inf
    ts = np.arange(a + 0.25, b, 0.5)
    d = np.sort(np.array([w_of(T, delta, t - t0, x, o, R) + o for t in ts]))
    k = int(math.ceil(0.5 / share * len(d) - 1e-9)) - 1
    return float(d[max(k, 0)])


def law_deficit(T, delta, t0, x, o, R, mu_over_lam, nw, sub=6, h=1e-3):
    """13.9.5 item 3, per 0.5 s window: (mu / lambda) |w'| / (1 + |w'|), w' the law's slope in time, averaged over the window."""
    out = np.zeros(nw)
    for k in range(nw):
        v = 0.0
        for i in range(sub):
            t = 0.5 * k + 0.5 * (i + 0.5) / sub
            if t - t0 > h:
                wp = abs(w_of(T, delta, t - t0 + h, x, o, R) - w_of(T, delta, t - t0 - h, x, o, R)) / (2 * h)
                v += wp / (1.0 + wp)
        out[k] = mu_over_lam * v / sub
    return out


def tail_law(c, x, o, delta, t_ov):
    """13.9.2: w t_ov = 2 (c / x_bar) s_b y / (1 + y) on a window's own inputs, s_b = (c + x_bar o) / delta, y = t_ov / s_b."""
    xb = max(x, 1.0)
    sb = (c + xb * o) / delta
    y = t_ov / sb
    return 2.0 * (c / xb) * sb * y / (1.0 + y)


# ---- reading helpers -------------------------------------------------------------------------------------------------------------------
def gauge_series(recs, col):
    """Per seed and 0.5 s window counts from the resend gauge (col 1 resends, 2 duplicate arrivals, 3 repairs sent); None when a run carries no gauge."""
    nw = len(recs[0]["series"]["handed"])
    arr = np.zeros((len(recs), nw))
    for i, r in enumerate(recs):
        ro = r.get("resend_outcome")
        if ro is None:
            return None
        for row in ro["series"]:
            k = int(round(row[0] / 0.5))
            if k < nw:
                arr[i, k] = row[col]
    return arr


def resend_term(a_recs, b_recs):
    """kappa_r (extra resends of B' over A per window) / (units handed per window), seed means; without the gauge, the whole-run extra resends spread evenly over the windows."""
    nw = len(a_recs[0]["series"]["handed"])
    handed = np.mean([r["series"]["handed"] for r in a_recs], axis=0).astype(float)
    ga, gb = gauge_series(a_recs, 1), gauge_series(b_recs, 1)
    if ga is not None and gb is not None:
        return KAPPA_R * (gb.mean(axis=0) - ga.mean(axis=0)) / np.maximum(handed, 1.0), True
    dn = float(np.mean([r["gauges"]["repair_sends"] for r in b_recs]) - np.mean([r["gauges"]["repair_sends"] for r in a_recs]))
    return np.full(nw, KAPPA_R * dn / max(float(handed.sum()), 1.0)), False


def centred_mean(v, k):
    idx = [u for u in (k - 1, k, k + 1) if 0 <= u < len(v)]
    return float(np.mean([v[u] for u in idx]))


def pre_floor(aa0, aa1, ids, n):
    """Item 2's bar: the p95 over the first n seeds of |A - A'| in the share, averaged over the same windows."""
    sa, sb = window_shares_of(aa0), window_shares_of(aa1)
    v = [abs(float(np.nanmean(sa[s][ids] - sb[s][ids]))) for s in range(min(n, len(aa0), len(aa1)))]
    return float(np.percentile(v, 95)) if v else None


def window_shares_of(recs):
    return K.window_shares(recs)


def overload_inputs(b_recs, a_recs, T):
    """The run's own inputs of the law, from the stored gauges: x_hat (rows with t_ov > 0), c, R, t0, mu / lambda."""
    xs, cs, ns = [], [], []
    for r in b_recs:
        for row in r["rev4"]["cap_series"]:
            if row[2] is not None and row[4] is not None and row[5] > 0 and row[6]:
                xs.append(row[4])
                cs.append(row[6])
                ns.append(row[1])
    x = float(np.average(xs, weights=ns)) if xs else REG["x"]
    c = float(np.average(cs, weights=ns)) if cs else T - REG["R"]
    t0s = [r["rev4"]["ov_t0"] for r in b_recs if r["rev4"]["ov_t0"] is not None]
    mus = [r["rev4"]["dhat_mean"] for r in b_recs if r["rev4"]["dhat_mean"]]
    lam = (a_recs[0]["units_total"] - 1) / a_recs[0]["last_hand"]
    return dict(x=x, c=c, R=T - c, t0=float(np.mean(t0s)) if t0s else REG["t0"], mu=float(np.mean(mus)) if mus else float("nan"), lam=lam)


def band_word(v, pred, band):
    return "-" if v is None else ("inside" if abs(v - pred) <= band else "OUTSIDE")


# ---- the corrected tests of 13.9.5 ---------------------------------------------------------------------------------------------------------
def corrected_tests(cell, delta, T, a_recs, b_recs, aa_pair, pred):
    """Items 1-4 of 13.9.5 at one lifetime, p50 on the reader's convention and the bill, each beside its registered prediction (pred may be None).
    Returns (lines, flags {name: True / False / None}, s_b measured)."""
    n = min(len(a_recs), len(b_recs))
    a_recs, b_recs = a_recs[:n], b_recs[:n]
    L, ok = [], {}
    r4s = [r["rev4"] for r in b_recs]
    ots = [r["onset_t"] for r in r4s if r["onset_t"] is not None]
    sbs = [r["onset_tov"] for r in r4s if r["onset_tov"] is not None]
    if not ots:
        return ["T=%s: B' never reached the onset (onset_t None at every seed)" % B.TAG(T)], dict(item1=False, item2=None, item3=None, item4=None, p50=None, bill=None), None
    onset_t, sb = float(np.mean(ots)), float(np.mean(sbs))
    inp = overload_inputs(b_recs, a_recs, T)
    p = pred or {}
    sb_reg = s_b_of(T, delta)
    ok["item1"] = abs(sb / (p["s_b"] if p else sb_reg) - 1.0) <= BANDS["s_b"]
    L.append("- item 1, onset: s_b = %.2f s (seed mean of %d; registered %s; law on the registered inputs %.2f), onset_t = %.2f s (registered %s): %s" % (
        sb, len(ots), "%.2f" % p["s_b"] if p else "-", sb_reg, onset_t, "%.2f" % p["onset"] if p else "-", "inside +-8 %" if ok["item1"] else "OUTSIDE +-8 %"))
    a, b, _ = CR.pair_window(a_recs[0], b_recs[0], C.dead_of(cell))
    sh_a, sh_b = window_shares_of(a_recs), window_shares_of(b_recs)
    deficit = np.nanmean(sh_a - sh_b, axis=0)
    nw = len(deficit)
    term, have_series = resend_term(a_recs, b_recs)
    ser = {}
    for r in b_recs:
        for row in r["rev4"]["cap_series"]:
            ser.setdefault(row[0], []).append(row)
    # item 2, before the onset
    pre = sorted(t for t in ser if a - 1e-9 <= t and t + 0.5 <= onset_t)
    ids = [int(round(t / 0.5)) for t in pre]
    if len(pre) < 3:
        ok["item2"] = None
        L.append("- item 2, before the onset: %d windows inside the A-paired window (from %.1f s) < 3: n/a" % (len(pre), a))
    else:
        rows = [[x for x in ser[t] if x[2] is not None and x[6]] for t in pre]
        dev = max(abs(float(np.mean([x[2] * x[4] / x[6] for x in r])) - 1.0) for r in rows if r)
        d_meas = float(np.mean([deficit[i] for i in ids]))
        d_pred = float(np.mean([term[i] for i in ids]))
        floor = pre_floor(aa_pair[0], aa_pair[1], ids, n) if aa_pair else None
        ok["item2"] = None if floor is None else (dev <= BANDS["cap"] and abs(d_meas - d_pred) <= floor)
        L.append("- item 2, before the onset (%d windows from %.1f s, registered %s): cap x_hat / c within %.1f %% of 1 (bar 3 %%); deficit against A %.4f, "
                 "kappa_r x extra resends / handed %.4f (%s), excess %+.4f against the cell's own A/A floor %s (registered deficit %s): %s" % (
                     len(pre), a, p.get("pre", "-"), 100 * dev, d_meas, d_pred, "per-window gauge" if have_series else "whole-run spread", d_meas - d_pred,
                     "-" if floor is None else "%.4f" % floor, p.get("d2", "-"), {True: "holds", False: "OUTSIDE", None: "n/a (no A/A floor)"}[ok["item2"]]))
    # item 3, after the onset
    wins_after = [t for t in sorted(ser) if t >= onset_t - 0.5 and t >= a - 1e-9 and t + 0.5 <= b]
    if len(wins_after) < 3:
        ok["item3"] = None
        L.append("- item 3, after the onset: %d windows inside the window < 3: n/a" % len(wins_after))
    else:
        k_after = [int(round(t / 0.5)) for t in wins_after]
        law = law_deficit(T, delta, inp["t0"], inp["x"], REG["o"], inp["R"], inp["mu"] / inp["lam"], nw) + np.maximum(term, 0.0)
        peak = max(centred_mean(deficit, k) for k in k_after)
        peak_law = max(centred_mean(law, k) for k in k_after)
        ok["item3"] = abs(peak / peak_law - 1.0) <= BANDS["item3"]
        L.append("- item 3, after the onset: highest 3-window mean of the deficit %.4f against the law's value of that statistic %.4f (law + resend term; mu %.0f, lambda %.0f, x_hat %.3f, c %.3f, t0 %.2f), "
                 "ratio %.2f (bar +-15 %%), registered %s: %s" % (peak, peak_law, inp["mu"], inp["lam"], inp["x"], inp["c"], inp["t0"], peak / peak_law, p.get("item3", "-"), "holds" if ok["item3"] else "OUTSIDE"))
    # item 4, the tail
    errs = []
    for t in sorted(ser):
        rows = [x for x in ser[t] if x[2] is not None and x[6]]
        if rows and t >= a - 1e-9 and t + 0.5 <= b and float(np.mean([x[5] for x in rows])) >= 2.0 * sb:
            w, tov, x_, c_ = (float(np.mean([x[i] for x in rows])) for i in (2, 5, 4, 6))
            errs.append(w * tov / tail_law(c_, x_, REG["o"], delta, tov) - 1.0)
    if errs:
        e = np.abs(errs)
        ok["item4"] = float(e.max()) <= BANDS["item4"]
        L.append("- item 4, tail: w x t_ov against 2 (c / x_bar) s_b y / (1 + y) over %d windows with t_ov >= 2 s_b (registered %s): median |error| %.2f %%, max %.2f %% (bar 3 %%): %s" % (
            len(errs), p.get("tail", "-"), 100 * float(np.median(e)), 100 * float(e.max()), "holds" if ok["item4"] else "OUTSIDE"))
    else:
        ok["item4"] = None
        L.append("- item 4, tail: no window with t_ov >= 2 s_b = %.1f s inside the window: n/a" % (2 * sb))
    # the delay, the share and the bill
    xs, ys = C.steady_arrays(cell, b_recs, a_recs, n)
    sh, ot, p50 = float(np.nanmean(xs["share"])), float(np.nanmean(xs["ot"])), float(np.nanmean(np.where(np.isfinite(xs["p50"]), xs["p50"], np.nan)))
    mp50 = model_p50(T, delta, a, b, sh)
    L.append("- steady share (A pairing) %.4f (registered %s +- 0.002: %s); on time %.4f (registered %s); p50 %.3f s (reader's convention; registered %s +- %s: %s; the model on the measured share %.3f s)" % (
        sh, p.get("share", "-"), band_word(sh, p["share"], BANDS["share"]) if p else "-", ot, p.get("ot", "-"), p50, p.get("p50", "-"), p.get("p50_band", "-"),
        band_word(p50, p["p50"], p["p50_band"]) if p else "-", mp50))
    ok["share"] = abs(sh - p["share"]) <= BANDS["share"] if p else None
    ok["p50"] = abs(p50 - p["p50"]) <= p["p50_band"] if p else None
    d_bill = float(np.mean([(ra["share_any"] - rb["share_any"]) * ra["units_total"] for ra, rb in zip(a_recs, b_recs)]))
    fl = []
    for r in b_recs:
        cs = r["rev4"].get("cap_series") or []
        if cs and r["rev4"].get("dhat_mean") and cs[-1][6] is not None and cs[-1][2] is not None:
            fl.append(r["rev4"]["dhat_mean"] * max(cs[-1][6] - cs[-1][4] * cs[-1][2], 0.0))
    dn = float(np.mean([rb["gauges"]["repair_sends"] - ra["gauges"]["repair_sends"] for ra, rb in zip(a_recs, b_recs)]))
    if fl:
        pred_bill = float(np.mean(fl)) + KAPPA_R * dn
        ok["bill"] = abs(d_bill - pred_bill) <= BANDS["bill"]
        L.append("- the bill: A - B' = %.0f units; mu_hat (c - x_hat w_end)+ = %.0f, kappa_r x extra resends (%.0f) = %.0f, sum %.0f (registered %s), difference %+.0f (bar +-250): %s" % (
            d_bill, float(np.mean(fl)), dn, KAPPA_R * dn, pred_bill, p.get("bill", "-"), d_bill - pred_bill, "holds" if ok["bill"] else "OUTSIDE"))
    else:
        ok["bill"] = None
        L.append("- the bill: A - B' = %.0f units; no last overload window to evaluate the fluid term (n/a)" % d_bill)
    return L, ok, sb


def tests_for(cell, delta, jobs, aa_pair, preds):
    """The corrected tests at every finite lifetime present in both builds.  Returns (lines, flags, s_b by T)."""
    L, flags, sbs = [], {}, {}
    for bj in [j for j in jobs if K.is_bp(j) and j.key[1] != INF and j.key[2] == 0.0]:
        aj = [j for j in jobs if j.design == "A" and j.key == bj.key]
        if not aj or not C.recs(aj[0]) or not C.recs(bj):
            continue
        T = bj.key[1]
        ls, ok, sb = corrected_tests(cell, delta, T, C.recs(aj[0]), C.recs(bj), aa_pair, preds.get(T))
        L.append("T=%s:" % B.TAG(T))
        L += ls
        for k, v in ok.items():
            flags["%s@%s" % (k, B.TAG(T))] = v
        sbs[T] = sb
    if sbs.get(2.0) and sbs.get(0.4):
        ratio = sbs[2.0] / sbs[0.4]
        flags["ratio"] = abs(ratio / RATIO_PRED - 1.0) <= BANDS["ratio"]
        L.append("item 1, linearity: s_b(T = 2) / s_b(T = 0.4) = %.2f against %.2f +- 10 %%: %s" % (ratio, RATIO_PRED, "holds" if flags["ratio"] else "OUTSIDE"))
    return L, flags, sbs


def predictions_table():
    L = ["| T | s_b | onset_t | pre-onset windows | item 2 deficit | item 3 statistic | tail windows | steady share | on time (0.4 s) | p50, A pairing | A - B' whole run |", "|---|---|---|---|---|---|---|---|---|---|---|"]
    for T, p in PRED.items():
        L.append("| %s | %.2f | %.2f | %d | %s | %.3f | %d | %.3f | %s | %.3f s | %d |" % (B.TAG(T), p["s_b"], p["onset"], p["pre"], p["d2"], p["item3"], p["tail"], p["share"], p["ot"], p["p50"], p["bill"]))
    return L


def read_q(jobs, nf, double=frozenset()):
    """OV-long-q: the gauge table and dominance reading (confirm13's), then the corrected tests of 13.9.5 beside the predictions of 13.9.6."""
    lines, undet, best = K.read_ov(Q, jobs, nf, double)
    aa = aa_q()
    aa_pair = (C.recs(aa[0]), C.recs(aa[1])) if all(C.recs(j) for j in aa) else None
    lines.append("\nRegistered predictions of B' (13.9.6, written before any run):\n")
    lines += predictions_table()
    lines.append("\nThe corrected tests (13.9.5) and the registered predictions, per lifetime (item 2's bar is the p95 of |A - A'| over the same windows from the cell's own A/A pair, 12 seeds, at the comparison's seed count):\n")
    tl, flags, _ = tests_for(Q, Q_DELTA, jobs, aa_pair, PRED)
    lines += tl
    bad = sorted(k for k, v in flags.items() if v is False)
    na = sorted(k for k, v in flags.items() if v is None)
    lines.append("\nThe cell confirms the corrected tests if items 1, 3 and 4 hold at every T, item 2 is within its floor at every T, and the bill and p50 are in their bands: %s%s" % (
        "all hold" if not bad else "OUTSIDE: " + ", ".join(bad), "; not read (n/a): " + ", ".join(na) if na else ""))
    lines.append("The residual's test is item 2 at all four T: a reading of 0.0018 within the floor at T = 1 and 2 means OV-long-m's excess was specific to that cell; about 0.0035-0.005 means the delta-specific "
                 "excess is real and grows with c / x (it goes into the derivation owed in 13.9.5); at T = 0.4 and 0.6 a reading like OV-long-m's 0.0013 is the unprotected excess recurring.")
    lines += reading_words(best, jobs)
    return lines, undet, best


def reading_words(best, jobs):
    """13.9.6's registered reading: the best point and the two words, printed beside the predictions."""
    L = ["\nRegistered reading (13.9.6) against the outcome:\n"]
    for design, label, pred_txt in (("rt", "stored arm", "share -0.009 +- 0.002, p50 +0.012 +- 0.015 s; predicted 'worse' (or 'equivalent' at the low end of both bands)"),
                                    ("rteq", "engine-queue arm", "share -0.010, p50 -0.55 s, on time +0.80; predicted 'a trade'")):
        rj = [j for j in jobs if j.design == design]
        bd = best.get(("B'", rj[0].name)) if rj else None
        if not bd:
            L.append("- against the %s: no reading" % label)
            continue
        xs, ys = bd["xs"], bd["ys"]
        L.append("- against the %s: best point T=%s (registered 0.4); share %+.4f, on time %+.4f, p50 %+.3f s; word: %s. Registered: %s" % (
            label, B.TAG(bd["T"]), float(np.mean(xs["share"] - ys["share"])), float(np.mean(xs["ot"] - ys["ot"])), float(np.mean(xs["p50"] - ys["p50"])), bd["word"], pred_txt))
    return L


# ---- RESEND-GAUGE (reported only) -----------------------------------------------------------------------------------------------------------
CLASSES = ("filled", "dup", "lost", "late", "other", "open")


def outcome_mean(recs, key):
    return float(np.mean([r["resend_outcome"][key] for r in recs]))


def read_gauge(jobs):
    L = ["\n## RESEND-GAUGE (reported only): the stored A and B' rows of OV-long-v and OV-long-m rerun with the resend gauge on, %d seeds\n" % GAUGE_SEEDS,
         "Every resend is classed once: filled (a hole), dup (the unit was already received), lost (random or outage loss, or a bottleneck-queue drop: qdrop is the queue's share), "
         "late (arrived after its hole was passed; floor is the floor's share), other (the unit was not yet a hole: ahead of the receiver's highest seq), open (still in flight at the end). "
         "The classes sum to the resend count (the gauge's own n, equal to the stored `repair_sends`).\n"]
    stored = stored_pairs()
    bad = []
    for sj, gj in stored:
        for s in range(gj.nseeds):
            rs, rg = C.load(sj.fp, s), C.load(gj.fp, s)
            if rs is None or rg is None:
                continue
            d = C.ident_diff(rs, rg)
            if d:
                bad.append("%s seed %d: %s" % (sj.name, s, d))
    L.append("Identity of each rerun with its stored run (every id3e key; the gauge adds only its own block): %s\n" % ("identical in every pair" if not bad else "DIFFERENT: " + "; ".join(bad[:10])))
    L += ["| cell | T | build | seeds | resends | filled | dup | lost (qdrop) | late (floor) | other | open | filled / n | dup / n | lost / n | late / n | repairs sent | never delivered |",
          "|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|"]
    by = {}
    for j in jobs:
        rs = C.recs(j)
        if not rs or "resend_outcome" not in rs[0]:
            continue
        by[(j.cell, j.key[1], "B'" if K.is_bp(j) else "A")] = rs
        n = outcome_mean(rs, "n")
        f = lambda k: outcome_mean(rs, k)                                                      # noqa: E731
        L.append("| %s | %s | %s | %d | %.0f | %.0f | %.0f | %.0f (%.0f) | %.0f (%.0f) | %.0f | %.1f | %.3f | %.3f | %.3f | %.3f | %.0f | %.0f |" % (
            j.cell, B.TAG(j.key[1]), "B'" if K.is_bp(j) else "A", len(rs), n, f("filled"), f("dup"), f("lost"), f("qdrop"), f("late"), f("late_floor"), f("other"), f("open"),
            f("filled") / max(n, 1), f("dup") / max(n, 1), f("lost") / max(n, 1), f("late") / max(n, 1),
            float(np.mean([sum(x[3] for x in r["resend_outcome"]["series"]) for r in rs])), float(np.mean([r["never_delivered"] for r in rs]))))
    L.append("\nB' minus A (seed means over the seeds both have), per extra resend; the fill fraction is what 13.9.3 reads as 0.93 (Delta never_delivered / Delta resends):\n")
    L += ["| cell | T | extra resends | extra filled | extra dup | extra lost | extra late | extra other | extra repairs | unfilled per extra resend | A - B' never_delivered / extra resends |", "|---|---|---|---|---|---|---|---|---|---|---|"]
    for (cell, T, bld), rb in sorted(by.items(), key=lambda kv: (kv[0][0], kv[0][1])):
        if bld != "B'" or (cell, T, "A") not in by:
            continue
        ra = by[(cell, T, "A")]
        m = min(len(ra), len(rb))
        d = lambda k: float(np.mean([rb[s]["resend_outcome"][k] - ra[s]["resend_outcome"][k] for s in range(m)]))        # noqa: E731
        dn = d("n")
        drep = float(np.mean([sum(x[3] for x in rb[s]["resend_outcome"]["series"]) - sum(x[3] for x in ra[s]["resend_outcome"]["series"]) for s in range(m)]))
        dnd = float(np.mean([ra[s]["never_delivered"] - rb[s]["never_delivered"] for s in range(m)]))
        L.append("| %s | %s | %.0f | %.0f | %.0f | %.0f | %.0f | %.0f | %.0f | %s | %s |" % (
            cell, B.TAG(T), dn, d("filled"), d("dup"), d("lost"), d("late"), d("other"), drep,
            "-" if not dn else "%.3f" % (1.0 - d("filled") / dn), "-" if not dn else "%.3f" % (dnd / dn)))
    for cell, T in ((K.M, 0.4), (K.M, 2.0)):
        if (cell, T, "A") in by and (cell, T, "B'") in by:
            L.append("\nPer-second series of %s at T=%s, A and B' side by side, seed means over 1 s (resends sent, duplicate arrivals, repairs sent): locates 13.9.3's pre-onset excess (registered onset_t %.1f s)\n" % (cell, B.TAG(T), PRED_M_ONSET[T]))
            L += per_second(by[(cell, T, "A")], by[(cell, T, "B'")])
    return L


PRED_M_ONSET = {0.4: 5.3, 2.0: 21.3}                 # 13.5's registered onset_t at delta = 0.1


def per_second(ra, rb):
    L = ["| t (s) | resends A | resends B' | dup A | dup B' | repairs A | repairs B' |", "|---|---|---|---|---|---|---|"]
    m = min(len(ra), len(rb))
    sa = [gauge_series(ra[:m], c).mean(axis=0) for c in (1, 2, 3)]
    sb = [gauge_series(rb[:m], c).mean(axis=0) for c in (1, 2, 3)]
    nw = len(sa[0])
    for k in range(0, nw - 1, 2):
        cells = []
        for i in range(3):
            cells += ["%.0f" % (sa[i][k] + sa[i][k + 1]), "%.0f" % (sb[i][k] + sb[i][k + 1])]
        L.append("| %.0f | %s |" % (k * 0.5, " | ".join(cells)))
    return L


# ---- the chain --------------------------------------------------------------------------------------------------------------------------
def run_chain13q(workers, only=None):
    smoke = bool(C.SMOKE)
    os.makedirs(C.OUT, exist_ok=True)
    reg = dict(smoke=dict(C.SMOKE), out=C.OUT)
    for j in all_jobs13q() + K.all_jobs13():
        C.JOBS_BY_FP[j.fp] = j
    C.PEAKS.update(C.load_peaks())
    if stopped13q():
        print("STOP13Q marker present: not starting", flush=True)
        return 1
    pj = part_jobs()
    with mp.Pool(workers, initializer=init_worker13q, initargs=(reg,)) as pool:
        def todo(part):
            return not (only and part != only) and not C.done("DONE13Q-" + part)

        def gate(part, bad):
            if bad and smoke:
                print("smoke: gate failures (a real run stops here): %s" % "; ".join(bad[:5]), flush=True)
                append_results(["### %s: gate failures (smoke, not stopped)" % part] + ["- " + b for b in bad[:20]])
                return True
            if bad:
                stop13q("part %s: %s" % (part, "; ".join(bad[:10])))
                append_results(["\n### %s: STOPPED by a gate\n" % part] + ["- " + b for b in bad[:40]])
                return False
            return True

        if todo("AA-q"):
            print("[%s] A/A rows of %s" % (time.strftime("%H:%M:%S"), Q), flush=True)
            C.run_pool(pool, C.tasks_of(pj["AA-q"]), "AA-q")
            nf = freeze_nf13q(smoke)
            finish("AA-q", ["\n## OV-long-q (13.9.6) (%s): the A/A floor\n" % time.strftime("%Y-%m-%d"), "```", json.dumps(nf.get(Q, {}), indent=1), "```"])
        if todo("OV-long-q"):
            jobs = q_jobs()
            nf = freeze_nf13q(smoke)
            print("[%s] cell %s: %d new runs" % (time.strftime("%H:%M:%S"), Q, K.runs_of(pj["OV-long-q"])), flush=True)
            C.run_pool(pool, C.tasks_of(jobs), Q)
            if not gate(Q, K.absolutes([j for j in jobs if j.design in ("A", "B", "adr")])):
                return 1
            lines, undet, _ = K.reading(read_q, jobs, nf)
            if undet:
                K.double_arms(pool, jobs, undet, Q)
                lines, _, _ = K.reading(read_q, jobs, nf, double=frozenset(undet))
            finish("OV-long-q", ["\n## OV-long-q: build B' at delta = 0.118 (13.9.6)\n"] + lines)
        if todo("RESEND-GAUGE"):
            jobs = gauge_jobs()
            tasks = C.tasks_of(jobs)
            print("[%s] resend gauge: %d runs" % (time.strftime("%H:%M:%S"), len(tasks)), flush=True)
            C.run_pool(pool, tasks, "RESEND-GAUGE")
            lines = K.reading(read_gauge, jobs)
            finish("RESEND-GAUGE", lines if isinstance(lines, list) else lines[0])
    if not only:
        C.marker("DONE13Q")
    return 0


def main(argv=None):
    ap = argparse.ArgumentParser()
    ap.add_argument("--workers", type=int, default=9, help="worker processes (capped at %d)" % MAX_WORKERS)
    ap.add_argument("--only", default=None, choices=PARTS)
    ap.add_argument("--smoke", action="store_true", help="exercise the whole chain on a fraction of the units, 3 seeds, in a scratch out dir")
    ap.add_argument("--smoke-out", default=None, help="reuse a smoke directory")
    ap.add_argument("--smoke-scale", type=float, default=K.SMOKE_SCALE)
    ap.add_argument("--counts", action="store_true", help="print the planned run counts")
    a = ap.parse_args(argv)
    B.build()
    if a.counts:
        reg, fresh, miss, todo = counts()
        for p, n in reg.items():
            print("%-13s %d" % (p, n))
        print("total registered new runs: %d (of which %d are ADR-0084 / Bulk rows at delta = 0.118 whose fingerprint differs from OV-long's)" % (sum(reg.values()), fresh))
        print("stored rows read for the gauge identity check and missing from the store: %d; new runs still to run: %d" % (miss, todo))
        return 0
    if a.smoke:
        import tempfile
        C.SMOKE.update(scale=a.smoke_scale, seeds=K.SMOKE_SEEDS)
        C.OUT = a.smoke_out or tempfile.mkdtemp(prefix="confirm13q_smoke_")
        global RESULTS
        RESULTS = os.path.join(C.OUT, "results-smoke.md")
        print("smoke run in", C.OUT, flush=True)
    return run_chain13q(max(1, min(a.workers, MAX_WORKERS)), a.only)


if __name__ == "__main__":
    mp.freeze_support()
    sys.exit(main())
