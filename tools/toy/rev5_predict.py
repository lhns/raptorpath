"""Revision 5 predictor (three-knobs-revision4.md 14): build B'' read from the stored B' runs, before any B'' run.  Reads only.

    CONFIRM_OUT=<store> python rev5_predict.py [calib]

For each stored B' run of OV-long-m / OV-long-q (and OV-long) it rebuilds the per-window series that build B'' would give, by two corrections taken
from the law and the measured rates, and reads them with the registered reader (confirm_read) against the stored rows of today's Realtime and of A:

  1. the cap: per dequeue window the run's own inputs (x_hat, t_ov, c+, and o_hat recovered from y) give w1 (R1, B') and w5 (the new ramp); a unit handed over at h
     is dequeued at tau = h + w(tau); its delay moves by w5(tau5) - w1(tau1), and the units delivered per hand-over window scale by (1 - w1'(tau1)) / (1 - w5'(tau5))
     (the descent's shed in hand-over time, 13.10.1);
  2. the repair, on 13.10.1's hole account: after the run's first intake shed every repair B' sent becomes a first send delivered at 1 - p0 (the net clamp gives
     r_eff = 0 while the intake sheds); B's decodes are lost, except those whose unit a resend also reached (the duplicates B' has above A at the same seed, which a
     resend now fills).  Added rate = [P (1 - p0) - dec + (dup_B' - dup_A)] / t_last.  With --hi, the decode loss is 0.5/s only (the upper edge of the band).

`calib` runs the same transform from the stored B' rows at delta = 0.1 to the law at delta = 0.118 (R1 kept, no repair term) and prints it beside the measured
B' rows at 0.118: the check that the cap transform reproduces a measured row before it is used for a prediction.
"""
import math
import os
import sys

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import batch3e as B  # noqa: E402
import confirm13 as K  # noqa: E402
import confirm13q as KQ  # noqa: E402
import confirm3e as C  # noqa: E402
import confirm_read as CR  # noqa: E402

INF = math.inf
P0 = 0.03
DEC_ONLY = 0.5                     # units/s that only a decode recovers (13.10.1, B' before the onset at T = 0.4); an upper bound after the onset


def r1(cx, y):
    return cx * min(1.0, 2.0 / (1.0 + y))


def r5(cx, y):
    return cx / max(1.0, y)


def law_windows(run, delta_new=None):
    """Per dequeue window: (time, c/xbar, y_old, y_new).  o_hat is recovered from the run's own y where the ramp acts; elsewhere the run's median is used."""
    cs = [w for w in run["rev4"]["cap_series"] if w[2] is not None and w[6] is not None]
    rows, os_ = [], []
    d0 = run["delta"]
    for t, n, w, age, x, tov, cp, n_on in cs:
        xb = max(1.0, x)
        cx = cp / xb
        y = 2.0 * cx / w - 1.0 if w < cx * (1 - 1e-9) else None
        if y is not None and y > 1.0 and tov > 0:
            os_.append((d0 * tov / y - cp) / xb)
        rows.append([t + 0.25, cx, y, tov, cp, xb])
    o = float(np.median(os_)) if os_ else 0.095
    out = []
    for t, cx, y, tov, cp, xb in rows:
        y0 = d0 * tov / (cp + xb * o)
        yn = (delta_new if delta_new is not None else d0) * tov / (cp + xb * o)
        out.append((t, cx, y0, yn))
    return out


def cap_fn(wins, form, which):
    ts = np.array([w[0] for w in wins])
    vs = np.array([form(w[1], w[2] if which == 0 else w[3]) for w in wins])
    return lambda t: float(np.interp(t, ts, vs))


def dequeue_time(h, wf):
    tau = h + wf(h)
    for _ in range(30):
        tau = h + wf(tau)
    return tau


def shift_hist(hist, dw, scale):
    out = {}
    for b, c in hist:
        v = CR.bin_value(b) + dw
        nb = CR.hist_bin(max(v, 1e-3))
        out[nb] = out.get(nb, 0.0) + c * scale
    return [[b, c] for b, c in out.items()]


def transform(run, old_form, new_form, delta_new=None, repair=True, a_run=None):
    ser = run["series"]
    wins = law_windows(run, delta_new)
    w_old = cap_fn(wins, old_form, 0)
    w_new = cap_fn(wins, new_form, 1)
    t_first_shed = run["rev4"].get("first_deq_exp") or 0.0
    t_end = run["last_hand"]
    rep_rate = run["gauges"]["rep_sent"] / max(t_end, 1.0)
    if a_run is not None and not HI:
        g, ga = run["gauges"], a_run["gauges"]
        add_rate = max((g["rep_sent"] * (1.0 - P0) - g["dec_units"] + (g["dup"] - ga["dup"])) / max(t_end, 1.0), 0.0) if repair else 0.0
    else:
        add_rate = max(rep_rate * (1.0 - P0) - DEC_ONLY, 0.0) if repair else 0.0
    new = dict(W=ser["W"], handed=list(ser["handed"]), deliv=[], ot={k: [] for k in ser["ot"]}, hist=[])
    eps = 0.01
    for j, (H, D) in enumerate(zip(ser["handed"], ser["deliv"])):
        h = (j + 0.5) * ser["W"]
        t1, t5 = dequeue_time(h, w_old), dequeue_time(h, w_new)
        d1 = (w_old(t1 + eps) - w_old(t1 - eps)) / (2 * eps)
        d5 = (w_new(t5 + eps) - w_new(t5 - eps)) / (2 * eps)
        rho = (1.0 - d1) / (1.0 - d5)
        dw = w_new(t5) - w_old(t1)
        add = add_rate * ser["W"] if (repair and t1 > t_first_shed and h < t_end) else 0.0
        Dn = D * rho + add
        sc = Dn / D if D > 0 else 0.0
        hs = shift_hist(ser["hist"][j], dw, sc)
        new["deliv"].append(Dn)
        new["hist"].append(hs)
        for k in ser["ot"]:
            tau = float(k)
            new["ot"][k].append(sum(c for b, c in hs if CR.bin_value(b) <= tau + 1e-9))
    out = dict(run)
    out["series"] = new
    return out


def read_vs(new_recs, base_recs, bars):
    """Per-seed paired readouts of new against base (the first min(n) seeds); returns mean differences (new - base; p50 sign: base - new positive favours new) and words."""
    n = min(len(new_recs), len(base_recs))
    ds = {"share": [], "ot": [], "p50": []}
    lev = {"share": [], "p50": []}
    for s in range(n):
        pr = CR.paired(new_recs[s], base_recs[s], None)
        rx, ry = pr
        ds["share"].append(rx["share"] - ry["share"])
        ds["ot"].append(rx["ot"] - ry["ot"])
        ds["p50"].append(ry["p50"] - rx["p50"])
        lev["share"].append(rx["share"])
        lev["p50"].append(rx["p50"])
    words = {k: CR.read_good(np.array(v), bars[k])["word"] for k, v in ds.items()}
    return {k: float(np.mean(v)) for k, v in ds.items()}, words, {k: float(np.mean(v)) for k, v in lev.items()}


def jobs_for(cell):
    if cell == KQ.Q:
        js = KQ.q_jobs()
    elif cell == K.M:
        js = K.m_jobs()
    else:
        js = C.jobs_of(cell) + K.ov_new(cell)
    return js


def rows(cell, T):
    js = jobs_for(cell)
    key = ("lt", T, 0.0)
    bp = [j for j in js if j.design == "B" and j.key == key and (" | Bp | " in j.name)][0]
    a = [j for j in js if j.design == "A" and j.key == key][0]
    d = KQ.Q_DELTA if cell == KQ.Q else (K.M_DELTA if cell == K.M else 50.0)
    rt = [j for j in js if j.design == "rt" and j.key == ("rt", d)][0]
    rteq = [j for j in js if j.design == "rteq" and j.key == ("rt", d)][0]
    return C.recs(bp), C.recs(a), C.recs(rt), C.recs(rteq)


BARS = dict(share=0.0096, ot=0.0096, p50=0.0086)


def predict(cell, Ts=(0.4, 0.6, 1.0, 2.0)):
    print("== %s" % cell)
    for T in Ts:
        bp, a, rt, rteq = rows(cell, T)
        bpp = [transform(r, r1, r5, a_run=a[s] if s < len(a) else None) for s, r in enumerate(bp)]
        for lab, base in (("rt", rt), ("rteq", rteq)):
            d0, w0, l0 = read_vs(bp, base, BARS)
            d1, w1, l1 = read_vs(bpp, base, BARS)
            print("T=%-4s vs %-4s  B' share %+.4f p50(base-new) %+.4f [%s/%s/%s] | B'' share %+.4f ot %+.4f p50 %+.4f [%s/%s/%s]  B'' p50 %.3f share %.4f"
                  % (T, lab, d0["share"], d0["p50"], w0["share"], w0["ot"], w0["p50"], d1["share"], d1["ot"], d1["p50"], w1["share"], w1["ot"], w1["p50"], l1["p50"], l1["share"]))
        da, _, la = read_vs(bpp, a, BARS)
        print("          B'' - A share %+.4f  p50(A - B'') %+.4f; whole-run B'' delivered %.4f (B' %.4f)" % (
            da["share"], da["p50"], np.mean([sum(r["series"]["deliv"]) / sum(r["series"]["handed"]) for r in bpp]),
            np.mean([sum(r["series"]["deliv"]) / sum(r["series"]["handed"]) for r in bp])))


def calib():
    """B' at delta = 0.1 transformed to the law at 0.118 (R1 kept, no repair term), against the measured B' at 0.118, both read against their own cell's rt."""
    print("== calibration: B'(0.1) -> R1 at 0.118 against measured B'(0.118)")
    for T in (0.4, 0.6, 1.0, 2.0):
        bm, am, rtm, _ = rows(K.M, T)
        bq, aq, rtq, _ = rows(KQ.Q, T)
        pred = [transform(r, r1, r1, delta_new=KQ.Q_DELTA, repair=False) for r in bm]
        dp, _, lp = read_vs(pred, rtq, BARS)
        dq, _, lq = read_vs(bq, rtq, BARS)
        dm, _, lm = read_vs(bm, rtm, BARS)
        print("T=%-4s measured 0.1: share %+.4f p50 %.3f | predicted 0.118: share %+.4f p50 %.3f | measured 0.118: share %+.4f p50 %.3f" % (
            T, dm["share"], lm["p50"], dp["share"], lp["p50"], dq["share"], lq["p50"]))


HI = "--hi" in sys.argv

if __name__ == "__main__":
    if sys.argv[1:] == ["calib"]:
        calib()
    else:
        for cell in (K.M, KQ.Q):
            predict(cell)
        predict("OV-long", Ts=(0.4, 2.0))
