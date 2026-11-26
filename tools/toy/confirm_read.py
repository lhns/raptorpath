"""The confirmation's reader (three-knobs-revision4.md 12.2, 12.3).  Pure functions on stored run records; no simulation here.

A run record carries `series` (per 0.5 s hand-over window: units handed over, delivered, delivered within tau, and a 1 %-resolution histogram of the delays of
the delivered units) and `rev4` (the first RTT sample and the four queue gauges: the first dequeue expiry, the first FIFO-full, the first link-queue drop, the first
store pause).  From them:

  * the steady window of an arm (12.2): start = max(fill, first RTT sample) + 1 s, end = last hand-over - tau; one window per comparison and per seed (the later of
    the two arms' starts, the common end), minus [outage start, the later recovery end] in an outage cell;
  * the three steady readouts (delivered share, share on time within tau, delay p50 from hand-over over every unit handed over, inf when fewer than half arrive);
  * the bars (12.2): the two shares max(0.05 (1 - share_ref), 1/units, NF), the delay max(NF_p50, 0.05 p50_ref), recovery max(0.05 ref, 0.1 s, NF);
  * the A/A floors (NF) from build A against its phase variant, p95 over the first n seeds of |A - A'|, each on the paired window;
  * the matched-share reading (headroom and outage cells) and the dominance reading (overload cells; best finite-T point = the highest seed-mean steady on-time share,
    ties within its bar broken by the lower p50, then by the smaller T).
"""
import math

import numpy as np

from proof_s1 import curve_at, merge_points, read_good  # noqa: F401  (proof stage 1's reading primitives)

W = 0.5
LOGB = math.log(1.01)
REC_CAP = 14.5
FILL_KEY = {"A": "first_deq_exp", "B": "first_deq_exp", "aa": "first_deq_exp", "adr": "first_pause_t", "bulk": "first_pause_t", "rt": "first_qdrop_t", "rteq": "fq_full_t"}


# ---- the per-run series ------------------------------------------------------------------------------------------------------------
def hist_bin(delay):
    return int(math.floor(math.log(delay / 1e-3) / LOGB)) if delay >= 1e-3 else -1


def bin_value(b):
    return 1e-3 * math.exp((b + 0.5) * LOGB) if b >= 0 else 5e-4


TAUS = (0.4, 2.0)                                                                  # the two on-time thresholds of the confirmation's cells (a stored run is shared between cells of one configuration)


def tkey(tau):
    return "%g" % tau


def make_series(h, dl, units_total, dt, taus=TAUS, width=W):
    """h: hand-over times of the sent seqs, dl: their delivery times (-1 when never delivered); units never sent have no entry and count as handed over, late.
    Returns per-window lists: handed, deliv, ot and hist ([[bin, count], ...] over the delivered units)."""
    h = np.asarray(h, dtype=float)
    dl = np.asarray(dl, dtype=float)
    nw = int(math.ceil(units_total * dt / width - 1e-9))
    uw = np.minimum((np.arange(units_total) * dt / width + 1e-9).astype(int), nw - 1)
    handed = np.bincount(uw, minlength=nw)
    ws = np.minimum((h / width + 1e-9).astype(int), nw - 1)
    ok = dl >= 0
    delay = dl - h
    deliv = np.bincount(ws[ok], minlength=nw)
    ot = {tkey(t): np.bincount(ws[ok & (delay <= t + 1e-9)], minlength=nw).tolist() for t in taus}
    hist = [[] for _ in range(nw)]
    if ok.any():
        d = np.maximum(delay[ok], 1e-3 * 0.999)
        b = np.where(delay[ok] >= 1e-3, np.floor(np.log(np.maximum(delay[ok], 1e-3) / 1e-3) / LOGB).astype(int), -1)
        key = ws[ok].astype(np.int64) * 100000 + (b + 1)
        uk, cnt = np.unique(key, return_counts=True)
        for k, c in zip(uk, cnt):
            hist[int(k // 100000)].append([int(k % 100000) - 1, int(c)])
        del d
    return dict(W=width, handed=handed.tolist(), deliv=deliv.tolist(), ot=ot, hist=hist)


# ---- windows and steady readouts ---------------------------------------------------------------------------------------------------
def start_of(rec):
    """12.2: max(fill, first RTT sample) + 1 s; with no fill (the queue never fills) one second after the first RTT sample."""
    r4 = rec["rev4"]
    rtt = r4["first_rtt_t"]
    fill = r4.get(FILL_KEY[rec["confirm_design"]])
    return (max(fill, rtt) if fill is not None else rtt) + 1.0


def end_of(rec):
    return rec["last_hand"] - rec["tau"]


def recovery_end(rec, dead):
    if dead is None:
        return None
    rt = rec.get("recovery_time")
    return math.inf if rt is None else dead[1] + dead[2] + rt


def pair_window(x, y, dead):
    """The paired steady window of two records of one seed: (start, end, exclusion or None)."""
    a = max(start_of(x), start_of(y))
    b = min(end_of(x), end_of(y))
    ex = None
    if dead is not None:
        ex = (dead[1], max(recovery_end(x, dead), recovery_end(y, dead)))
    return a, b, ex


def window_ids(n, a, b, ex, width=W):
    out = []
    for w in range(n):
        lo, hi = w * width, (w + 1) * width
        if lo >= a - 1e-9 and hi <= b + 1e-9 and not (ex and hi > ex[0] + 1e-9 and lo < ex[1] - 1e-9):
            out.append(w)
    return out


def readouts(series, ids, tau):
    """Steady share, on time within tau and p50 over the windows ids; None when the window is empty."""
    H = sum(series["handed"][w] for w in ids)
    if H <= 0:
        return None
    D = sum(series["deliv"][w] for w in ids)
    ot = series["ot"][tkey(tau)]
    O = sum(ot[w] for w in ids)
    counts = {}
    for w in ids:
        for b, c in series["hist"][w]:
            counts[b] = counts.get(b, 0) + c
    half = H / 2.0
    cum = 0
    p50 = math.inf
    for b in sorted(counts):
        cum += counts[b]
        if cum >= half:
            p50 = bin_value(b)
            break
    return dict(share=D / H, ot=O / H, p50=p50, handed=H)


def paired(x, y, dead):
    """The steady readouts of two records of one seed on their common window: (readouts of x, readouts of y), or None when the window is empty."""
    a, b, ex = pair_window(x, y, dead)
    ids = window_ids(len(x["series"]["handed"]), a, b, ex)
    rx, ry = readouts(x["series"], ids, x["tau"]), readouts(y["series"], ids, y["tau"])
    if rx is None or ry is None:
        return None
    return rx, ry


# ---- bars, NF ------------------------------------------------------------------------------------------------------------------------
def share_bar(ref, units, nf):
    return max(0.05 * (1.0 - ref), 1.0 / units, nf)


def p50_bar(ref, nf):
    return max(nf, 0.05 * ref) if math.isfinite(ref) else nf


def rec_bar(ref, nf):
    return max(0.05 * abs(ref), 0.1, nf)


NF_KEYS = ("share", "ot", "p50", "recovery", "wshare", "wot")


def whole(series, tau):
    """The whole-run delivered share and share on time within tau (the outage cells' registered readouts, as in stage 3e)."""
    H = max(sum(series["handed"]), 1)
    return sum(series["deliv"]) / H, sum(series[ "ot"][tkey(tau)]) / H


def nf_from_aa(a_recs, aa_recs, dead, n):
    """p95 over the first n seeds of |A - A'| on each paired window, per steady readout, and of the recovery times (censored at REC_CAP)."""
    d = {k: [] for k in NF_KEYS}
    for s in range(min(n, len(a_recs), len(aa_recs))):
        pr = paired(a_recs[s], aa_recs[s], dead)
        if pr is None:
            continue
        rx, ry = pr
        for k in ("share", "ot", "p50"):
            v = abs(rx[k] - ry[k]) if math.isfinite(rx[k]) and math.isfinite(ry[k]) else (0.0 if rx[k] == ry[k] else math.inf)
            d[k].append(v)
        wa, wb = whole(a_recs[s]["series"], a_recs[s]["tau"]), whole(aa_recs[s]["series"], aa_recs[s]["tau"])
        d["wshare"].append(abs(wa[0] - wb[0]))
        d["wot"].append(abs(wa[1] - wb[1]))
        ra, rb = a_recs[s].get("recovery_time"), aa_recs[s].get("recovery_time")
        d["recovery"].append(abs((REC_CAP if ra is None else ra) - (REC_CAP if rb is None else rb)))
    return {k: (float(np.percentile(v, 95)) if v else 0.0) for k, v in d.items()}


# ---- the readings ----------------------------------------------------------------------------------------------------------------------
def _diff(x, y, sign):
    """sign * (x - y) elementwise, with an infinite value (a delay that never arrives) read as 1e9 in its own direction and two infinities as equal."""
    x, y = np.asarray(x, dtype=float), np.asarray(y, dtype=float)
    out = np.zeros(np.broadcast(x, y).shape)
    fin = np.isfinite(x) & np.isfinite(y)
    out = np.where(fin, sign * (np.where(np.isfinite(x), x, 0.0) - np.where(np.isfinite(y), y, 0.0)), out)
    out = np.where(~np.isfinite(x) & np.isfinite(y), sign * 1e9, out)
    out = np.where(np.isfinite(x) & ~np.isfinite(y), -sign * 1e9, out)
    return out


def compare_curve(nx, ny, kept, bx, by, sign, bar, clamp_low=True):
    """The new curve interpolated per seed at that seed's baseline share, against the baseline's value, signed so that positive favours the new model."""
    yi = curve_at(nx, ny, kept, bx, clamp_low)
    if np.any(np.isnan(yi)):
        return dict(word="outside the swept range", mean=float("nan"), lo=float("nan"), hi=float("nan"), m=bar, n_out=int(np.isnan(yi).sum()))
    rd = read_good(_diff(yi, by, sign), bar)
    rd["n_out"] = 0
    return rd


def compare_pair(x, y, sign, bar):
    """Per-seed paired differences x - y signed so that positive favours x (the dominance reading)."""
    return read_good(_diff(x, y, sign), bar)


def best_point(points):
    """Dominance's best finite-T point (dated 2026-11-26): the highest seed-mean steady on-time share; points within its bar of that are tied and the lower p50 wins,
    then the smaller T.  points: list of (label, T, {k: per-seed array}, bar_ot)."""
    top = max(p[2]["ot"].mean() for p in points)
    tied = [p for p in points if top - p[2]["ot"].mean() <= p[3]]
    tied.sort(key=lambda p: (float(np.mean(np.where(np.isfinite(p[2]["p50"]), p[2]["p50"], 1e9))), p[1]))
    return tied[0]


def dominance_word(rds):
    """rds: {readout: reading}.  Dominates = not worse beyond the bar on any readout and better beyond it on at least one."""
    words = [r["word"] for r in rds.values()]
    if any(w in ("undetermined", "n/a") for w in words):
        return "undetermined"
    if any(w == "shown worse" for w in words):
        return "not dominated (a trade)"
    if any(w == "shown better" for w in words):
        return "dominates"
    return "equivalent"
