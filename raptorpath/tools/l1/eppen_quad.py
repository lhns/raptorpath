#!/usr/bin/env python3
"""EPPEN'S CORRELATION CONDITION at N PATHS — the c9 quad scorer.

goal-gate "Eppen's Condition at c8" §4 pre-registered C9-1 … C9-4 against a
geometry that did not exist. This is the reader those bars are scored with.

WHY A NEW FILE AND NOT AN EDIT TO `eppen_corr.py`. That script is the
instrument the c7/c8 PARTIAL verdict was read off, and it hard-codes N = 2 in
three places (`pair_windows` pairs pid 0 against pid 1; `center_two_way` loops
`for pid in (0, 1)`; the per-path level block does the same). Widening it in
place would edit the instrument a committed verdict rests on. The precedent is
`abort_witness.py`'s: existing parsers stay byte-identical, new geometries get
a new module. So `eppen_corr.py` is untouched, and THE CROSS-CHECK BELOW IS
THE PRICE OF THAT — `--selfcheck` re-derives c7's and c8's numbers with this
file's N-path code and requires them to match the published ones, because a
generalization that does not reproduce the special case is a rewrite wearing
the name of a widening.

WHAT IS NEW AT N = 4, and it is not just "more paths".

  * **rho_bar, the MEAN PAIRWISE correlation.** At N = 2 there is one pair and
    "the correlation" is unambiguous. At N = 4 there are SIX, and Eppen's
    benefit `B(N, rho_bar) = 1 - sqrt((1 + (N-1)*rho_bar)/N)` is stated in
    terms of their mean. Every pair is printed beside the mean: a rho_bar that
    is an average of +0.9 and -0.3 is a different cell from one that is six
    +0.3s, and the pooled-variance identity cannot tell them apart.
  * **The algebraic FLOOR moves.** One flow split by a work-conserving
    scheduler against a binding total makes the per-path draws mechanically
    anti-correlated, and for N exchangeable series summing to a constant the
    mean pairwise correlation is pinned at `-1/(N-1)`: -1.000 at N = 2 but
    only **-0.333 at N = 4**. A rho_bar below the floor is not a strong
    result, it is a MODEL FAILURE (the series are not exchangeable), and this
    file reports it as one rather than passing it through as a number.
  * **CLASS-PAIRED correlations.** C9-3 predicts that at the heterogeneous
    quad the fast-slow pairwise rho EXCEEDS the fast-fast one. That is a
    statement about which pairs, so pairs are grouped by leg class (read from
    each path's own measured `rtprop`, not from the cell name) and the two
    group means are reported separately.

THE GRANULARITY, which is the reason this geometry was unscoreable until now.
At the shipped `ACKDIAG_WINDOW_US = 2 s` a ~10 s invocation gives FOUR windows
per rep — six pairwise correlations cannot be carried by four points, and §4
recorded the 250 ms window as a BLOCKING dependency rather than a refinement.
This file therefore REFUSES to score a quad ledger captured at the 2 s cadence
(`--window-us` states what the ledger is; the `win=` fields are checked
against it) instead of printing six meaningless numbers.

    usage: eppen_quad.py <ledger.log> [more.log ...] [--qdir DIR] [--json]
                         [--window-us N] [--selfcheck]
"""
import argparse
import glob
import itertools
import json
import math
import os
import re
import sys

# ── the ACKDIAG line grammar (src/net/ackdiag.rs report format) ──────────
# Identical to eppen_corr.py's: the gauge's line format did not change when
# its WINDOW became overridable, which is exactly why the window has to be
# carried alongside the ledger rather than inferred from it.
_PREFIX = re.compile(r"^ACKDIAG\s+(\S+)\s+rep=(\d+)\s")
_BODY = re.compile(r"\[ACKDIAG\]\s+p(\d+)\s+win=([\d.]+)s\s+acks=(\d+)/z=(\d+)")
_RATE = re.compile(r"rate_lr=(\d+)sym/s")
_X50 = re.compile(r"\bx\[p50=([\d.]+)")
_XANCH = re.compile(r"xanchor=([\d.]+)")
_RTPROP = re.compile(r"rtprop=([\d.]+)ms")
_ANCHOR = re.compile(r"anchor=(\d+)sym")

#: The shipped default, µs. A quad ledger at THIS cadence is unscoreable and
#: the script says so; see the module docstring.
SHIPPED_WINDOW_US = 2_000_000
#: The pre-registered c9 cadence, µs (`RWM_ACKDIAG_WINDOW_US=250000`).
C9_WINDOW_US = 250_000

SERIES = [
    ("rate_lr", "delivered rate  (sym/s)", "DEMAND  — pool drain"),
    ("acks", "ack arrivals    (count)", "DEMAND  — draw events"),
    ("zfrac", "zero-delta frac (of acks)", "STOCKOUT— starved wire"),
    ("x50", "over-read x p50", "SIGNAL  — sampler"),
]


def parse_lines(paths):
    """-> list of window records, one per (cell, rep, path, window index)."""
    recs = []
    seen = {}
    for p in paths:
        with open(p, "r", errors="replace") as fh:
            for line in fh:
                line = line.rstrip("\n")
                mp = _PREFIX.match(line)
                if not mp:
                    continue
                mb = _BODY.search(line)
                if not mb:
                    continue
                cell, rep = mp.group(1), int(mp.group(2))
                pid = int(mb.group(1))
                acks = int(mb.group(3))
                z = int(mb.group(4))
                key = (cell, rep, pid)
                widx = seen.get(key, 0)
                seen[key] = widx + 1

                def one(rx, cast=float, default=float("nan")):
                    m = rx.search(line)
                    return cast(m.group(1)) if m else default

                recs.append({
                    "cell": cell, "rep": rep, "pid": pid, "w": widx,
                    "win_s": float(mb.group(2)),
                    "acks": float(acks), "z": float(z),
                    "zfrac": (z / acks) if acks else float("nan"),
                    "rate_lr": one(_RATE), "x50": one(_X50),
                    "xanchor": one(_XANCH), "rtprop": one(_RTPROP),
                    "anchor": one(_ANCHOR),
                })
    return recs


def group_windows(recs, cell):
    """Group ALL paths of a cell on (rep, window index).

    THE N-PATH GENERALIZATION of `eppen_corr.py::pair_windows`, and it keeps
    that function's one load-bearing rule: a (rep, window) whose paths are not
    ALL present is DROPPED and counted, never imputed. At N = 2 that rule
    dropped a pair when either side was missing; at N = 4 it drops a window
    when any of the four is, which is stricter and deliberately so — a
    correlation computed over a window where one leg was silent is a
    correlation with a hole in it, and the hole is not visible downstream.

    Returns (pids, groups, dropped) where each group is a pid -> record dict
    covering exactly `pids`.
    """
    by = {}
    pids = set()
    for r in recs:
        if r["cell"] != cell:
            continue
        by[(r["rep"], r["w"], r["pid"])] = r
        pids.add(r["pid"])
    pids = sorted(pids)
    groups, dropped = [], 0
    for rep, w in sorted({(rep, w) for (rep, w, _) in by}):
        g = {pid: by.get((rep, w, pid)) for pid in pids}
        if any(v is None for v in g.values()):
            dropped += 1
            continue
        groups.append(g)
    return pids, groups, dropped


def pearson(xs, ys):
    n = len(xs)
    if n < 3:
        return None
    mx, my = sum(xs) / n, sum(ys) / n
    sxx = sum((x - mx) ** 2 for x in xs)
    syy = sum((y - my) ** 2 for y in ys)
    sxy = sum((x - mx) * (y - my) for x, y in zip(xs, ys))
    if sxx <= 0 or syy <= 0:
        return None
    return sxy / math.sqrt(sxx * syy)


def fisher_ci(r, n, z=1.96):
    if r is None or n < 4 or abs(r) >= 1.0:
        return (float("nan"), float("nan"))
    zr = 0.5 * math.log((1 + r) / (1 - r))
    se = 1.0 / math.sqrt(n - 3)
    return (math.tanh(zr - z * se), math.tanh(zr + z * se))


def stdev(vs):
    n = len(vs)
    if n < 2:
        return 0.0
    m = sum(vs) / n
    return math.sqrt(sum((v - m) ** 2 for v in vs) / (n - 1))


def series_raw(groups, pids, key):
    """pid -> the series, with any window holding a NaN on ANY path dropped
    from EVERY path. Dropping per-path would leave the series different
    lengths and silently mis-align them."""
    ok = [g for g in groups if not any(math.isnan(g[p][key]) for p in pids)]
    return {p: [g[p][key] for g in ok] for p in pids}, len(ok)


def center_by_rep(groups, pids, key):
    """Within-rep centering, N paths. Same rule as eppen_corr.py: a rep is one
    invocation with its own absolute level, and leaving the level in
    manufactures a between-rep correlation."""
    ok = [g for g in groups if not any(math.isnan(g[p][key]) for p in pids)]
    out = {p: [] for p in pids}
    reps = sorted({g[pids[0]]["rep"] for g in ok})
    for rep in reps:
        rows = [g for g in ok if g[pids[0]]["rep"] == rep]
        if not rows:
            continue
        for p in pids:
            vs = [g[p][key] for g in rows]
            m = sum(vs) / len(vs)
            out[p] += [v - m for v in vs]
    return out, len(ok)


def center_two_way(groups, pids, key):
    """Rep AND window-index effects removed, N paths — the classical two-way
    additive residual `v - rep_mean - win_mean + grand_mean`, computed per
    path exactly as `eppen_corr.py::center_two_way` does for two.

    ITS df IS NOT ITS n, and the same conservative choice is made here:
    removing R rep means and W window means leaves `(R-1)(W-1)` df, so the CI
    is taken at an effective n of `(R-1)(W-1)+1`. At the quad's ~40 windows
    per rep over 3 reps that is a far less punishing correction than it was at
    four windows, which is the whole reason the 250 ms cadence is a
    prerequisite rather than a refinement."""
    ok = [g for g in groups if not any(math.isnan(g[p][key]) for p in pids)]
    if not ok:
        return {p: [] for p in pids}, 0
    reps = sorted({g[pids[0]]["rep"] for g in ok})
    wins = sorted({g[pids[0]]["w"] for g in ok})
    out = {}
    for p in pids:
        vs = [g[p][key] for g in ok]
        grand = sum(vs) / len(vs)
        rm, wm = {}, {}
        for r in reps:
            sel = [v for v, g in zip(vs, ok) if g[pids[0]]["rep"] == r]
            rm[r] = sum(sel) / len(sel) if sel else grand
        for w in wins:
            sel = [v for v, g in zip(vs, ok) if g[pids[0]]["w"] == w]
            wm[w] = sum(sel) / len(sel) if sel else grand
        out[p] = [
            v - rm[g[pids[0]]["rep"]] - wm[g[pids[0]]["w"]] + grand
            for v, g in zip(vs, ok)
        ]
    n_eff = (len(reps) - 1) * (len(wins) - 1) + 1
    return out, n_eff


def pairwise(series, pids):
    """-> {(a,b): rho} over all C(N,2) unordered pairs, and the mean."""
    out = {}
    for a, b in itertools.combinations(pids, 2):
        out[(a, b)] = pearson(series[a], series[b])
    vals = [v for v in out.values() if v is not None]
    return out, (sum(vals) / len(vals) if vals else None)


def eppen_benefit_n(n, rho_bar):
    """`B(N, rho_bar) = 1 - sqrt((1 + (N-1)*rho_bar)/N)` — the pooled-variance
    identity for N EXCHANGEABLE series with equal sigmas, which is the form
    §4's predictions are stated in.

    `[SECONDARY: Eppen's closed form is paywalled and NOT CONSULTED; this is
    the pooled-variance identity, which is where the correlation dependence
    lives, not his cost expression.]`

    Returns None below the algebraic floor `-1/(N-1)`, where the radicand goes
    negative: that is not a large benefit, it is the model reporting that the
    series it was handed are not exchangeable."""
    if rho_bar is None:
        return None
    inner = (1.0 + (n - 1) * rho_bar) / n
    if inner < 0:
        return None
    return 1.0 - math.sqrt(inner)


def rho_floor(n):
    """`-1/(N-1)` — -1.000 at N = 2, -0.333 at N = 4.

    §4 introduces this as *the adding-up constraint's* floor: one flow split by
    a work-conserving scheduler against a binding total makes the per-path
    draws mechanically anti-correlated, and for N exchangeable series summing
    to a constant the mean pairwise correlation sits exactly here.

    **IT IS ALSO, AND MORE STRONGLY, AN ALGEBRAIC IDENTITY THAT NO MEASUREMENT
    CAN VIOLATE — which makes it useless as a falsifier.** Any sample
    correlation matrix `R` is positive semi-definite, so `1' R 1 >= 0`; that
    expands to `N + 2 * sum_{i<j} rho_ij >= 0`, i.e.

        rho_bar  =  (sum_{i<j} rho_ij) / C(N,2)  >=  -N / (2 * C(N,2))
                 =  -1/(N-1)

    for ANY N series whatever — exchangeable or not, equal-variance or not,
    adding up to a binding total or not. The bound is a property of the
    ESTIMATOR, not of our scheduler. See `test_eppen_quad.py`, which both
    proves it cannot be breached and shows it being approached from above by
    an exchangeable, sums-to-constant quad.

    The consequence is recorded where it bites, in the c9 contract: **C9-1's
    second falsification clause ("or rho_bar < -0.34") is UNSATISFIABLE as
    written.** No four-path measurement can produce it, so that half of the
    bar cannot discriminate between a right model and a wrong one."""
    return -1.0 / (n - 1) if n > 1 else float("nan")


def class_split(groups, pids):
    """Split the paths into leg CLASSES by their own measured RTprop, so C9-3's
    fast-slow vs fast-fast contrast is read off the WIRE rather than off the
    cell name.

    The split is at the midpoint of the observed RTprop range, and it is only
    taken when the range is wide enough to be a real split (a symmetric quad's
    four legs differ by jitter alone and must NOT be forced into two classes).
    Returns (fast_pids, slow_pids) or None when the cell is not separable."""
    mean_rtt = {}
    for p in pids:
        vs = [g[p]["rtprop"] for g in groups if not math.isnan(g[p]["rtprop"])]
        if not vs:
            return None
        mean_rtt[p] = sum(vs) / len(vs)
    lo, hi = min(mean_rtt.values()), max(mean_rtt.values())
    # A 2x spread is the heterogeneous quad's signature (c2 ~10 ms vs c3
    # ~40 ms RTT); a symmetric quad's legs sit within a few percent.
    if lo <= 0 or hi / lo < 2.0:
        return None
    mid = (lo + hi) / 2.0
    fast = sorted(p for p in pids if mean_rtt[p] < mid)
    slow = sorted(p for p in pids if mean_rtt[p] >= mid)
    if not fast or not slow:
        return None
    return fast, slow, mean_rtt


# ── the seed audit, widened to N legs ────────────────────────────────────
_QDEV = re.compile(r"^==\s+(CLI\d|SRV\d)")
_QSEED = re.compile(r"\bseed\s+(\d+)")
_QLOSS = re.compile(r"loss gemodel p ([\d.]+)% r ([\d.]+)%")
_QDELAY = re.compile(r"delay ([\d.]+)ms\s+([\d.]+)?ms?")
_QRATE = re.compile(r"rate (\d+\w+)")


def seed_audit(qdir):
    """Reads the `-q.txt` qdisc captures over ALL CLI legs, not just two.

    This is the instrument that found the shared-seed defect, and its verdict
    at a quad is the one that matters most: the symmetric quad is exactly the
    geometry where a shared seed would pin every pairwise loss correlation at
    +1 by construction. `distinct_seeds == n_legs` is the post-repair
    signature; `distinct_seeds == 1` is the old era."""
    out = []
    for path in sorted(glob.glob(os.path.join(qdir, "*-q.txt"))):
        dev, cur = None, {}
        for line in open(path, "r", errors="replace"):
            m = _QDEV.match(line.strip())
            if m:
                dev = m.group(1)
                continue
            if dev and line.strip().startswith("qdisc"):
                s = _QSEED.search(line)
                l = _QLOSS.search(line)
                d = _QDELAY.search(line)
                r = _QRATE.search(line)
                cur[dev] = {
                    "seed": s.group(1) if s else None,
                    "loss": (l.group(1), l.group(2)) if l else None,
                    "delay": d.group(1) if d else None,
                    "rate": r.group(1) if r else None,
                }
                dev = None
        legs = sorted(k for k in cur if k.startswith("CLI"))
        if len(legs) < 2:
            continue
        seeds = [cur[k]["seed"] for k in legs]
        params = [(cur[k]["loss"], cur[k]["delay"], cur[k]["rate"]) for k in legs]
        out.append({
            "capture": os.path.basename(path),
            "legs": legs,
            "n_legs": len(legs),
            "seeds": seeds,
            "distinct_seeds": len(set(seeds)),
            "all_seeds_equal": len(set(seeds)) == 1,
            "per_leg_seeds": len(set(seeds)) == len(seeds),
            "symmetric_params": len(set(map(str, params))) == 1,
        })
    return out


def score_cell(recs, cell, window_us):
    pids, groups, dropped = group_windows(recs, cell)
    n = len(pids)
    out = {"paths": pids, "n_paths": n, "windows": len(groups),
           "dropped": dropped, "series": {}}
    if n < 2 or not groups:
        out["unscoreable"] = "fewer than two paths, or no complete window"
        return out

    # THE CADENCE CHECK. Six pairwise correlations cannot be carried by four
    # windows per rep, and §4 recorded the 250 ms window as a BLOCKING
    # dependency. A quad ledger at the shipped 2 s cadence is refused rather
    # than scored — the alternative is six numbers that look like a result.
    med_win = sorted(g[pids[0]]["win_s"] for g in groups)[len(groups) // 2]
    out["median_window_s"] = med_win
    out["declared_window_us"] = window_us
    if abs(med_win - window_us / 1e6) > 0.5 * window_us / 1e6:
        out["window_mismatch"] = (
            f"ledger windows measure ~{med_win:.3f}s but --window-us says "
            f"{window_us/1e6:.3f}s — the ledger is not at the declared cadence"
        )
    reps = sorted({g[pids[0]]["rep"] for g in groups})
    out["reps"] = reps
    wpr = len(groups) // max(len(reps), 1)
    out["windows_per_rep"] = wpr
    n_pairs = n * (n - 1) // 2
    if n > 2 and wpr < 3 * n_pairs:
        out["underpowered"] = (
            f"{wpr} windows/rep against {n_pairs} pairwise correlations — "
            f"the 250 ms cadence (RWM_ACKDIAG_WINDOW_US=250000) is a BLOCKING "
            f"dependency of C9-1..4, not a refinement"
        )

    for key, name, side in SERIES:
        raw, n_raw = series_raw(groups, pids, key)
        ctr, n_ctr = center_by_rep(groups, pids, key)
        two, n_eff = center_two_way(groups, pids, key)
        p_raw, m_raw = pairwise(raw, pids)
        p_ctr, m_ctr = pairwise(ctr, pids)
        p_two, m_two = pairwise(two, pids)
        lo, hi = fisher_ci(m_ctr, n_ctr)
        lo2, hi2 = fisher_ci(m_two, n_eff)
        entry = {
            "side": side,
            "rho_bar_raw": m_raw,
            "rho_bar_centered": m_ctr,
            "rho_bar_2way": m_two,
            "n_centered": n_ctr,
            "n_eff_2way": n_eff,
            "ci95": [None if math.isnan(lo) else lo, None if math.isnan(hi) else hi],
            "ci95_2way": [None if math.isnan(lo2) else lo2,
                          None if math.isnan(hi2) else hi2],
            "pairs_raw": {f"p{a}-p{b}": v for (a, b), v in p_raw.items()},
            "pairs_centered": {f"p{a}-p{b}": v for (a, b), v in p_ctr.items()},
            "pairs_2way": {f"p{a}-p{b}": v for (a, b), v in p_two.items()},
            "floor": rho_floor(n),
            # NOT A FINDING CHANNEL — AN ARITHMETIC SELF-CHECK. `rho_floor`'s
            # docstring proves `rho_bar >= -1/(N-1)` holds for any sample
            # correlation matrix, so this can only fire on a BUG in this file
            # (a mis-paired series, a residual computed against the wrong
            # mean). It is kept because it is free and it is the one condition
            # under which every number on the line is worthless; it is
            # reported as an internal inconsistency, never as a measured
            # outcome. A tolerance is carried because the bound is exact in
            # real arithmetic and this is floating point.
            "below_floor_2way": (m_two is not None
                                 and m_two < rho_floor(n) - 1e-9),
            "B_centered": eppen_benefit_n(n, m_ctr),
            "B_2way": eppen_benefit_n(n, m_two),
        }
        if key in ("rate_lr", "acks"):
            sig = {p: stdev(ctr[p]) for p in pids}
            entry["sigma_sum"] = sum(sig.values())
            b = entry["B_centered"]
            entry["B_abs_centered"] = None if b is None else b * sum(sig.values())
        # C9-3's contrast: class-paired means, when the cell is separable.
        cs = class_split(groups, pids)
        if cs:
            fast, slow, mean_rtt = cs
            entry["classes"] = {"fast": fast, "slow": slow,
                                "rtprop_ms": {f"p{p}": v for p, v in mean_rtt.items()}}
            for lbl, prs in (("2way", p_two), ("centered", p_ctr)):
                ff = [v for (a, b), v in prs.items()
                      if v is not None and a in fast and b in fast]
                ss = [v for (a, b), v in prs.items()
                      if v is not None and a in slow and b in slow]
                fs = [v for (a, b), v in prs.items()
                      if v is not None and ((a in fast) != (b in fast))]
                entry[f"class_{lbl}"] = {
                    "fast_fast": sum(ff) / len(ff) if ff else None,
                    "slow_slow": sum(ss) / len(ss) if ss else None,
                    "fast_slow": sum(fs) / len(fs) if fs else None,
                }
        out["series"][key] = entry

    out["levels"] = {}
    for p in pids:
        vs = [g[p]["rate_lr"] for g in groups if not math.isnan(g[p]["rate_lr"])]
        rt = [g[p]["rtprop"] for g in groups if not math.isnan(g[p]["rtprop"])]
        if not vs:
            continue
        mu, sd = sum(vs) / len(vs), stdev(vs)
        out["levels"][f"p{p}"] = {
            "rate_lr_mean": mu, "rate_lr_sd": sd,
            "rate_lr_cv": sd / mu if mu else float("nan"),
            "rtprop_mean": (sum(rt) / len(rt)) if rt else float("nan"),
        }
    return out


def fmt(v):
    return "    n/a" if v is None else f"{v:>+7.3f}"


def report(result):
    for cell, c in result["cells"].items():
        n = c["n_paths"]
        print(f"--- {cell}   N={n} paths {c['paths']}   complete windows="
              f"{c['windows']}" + (f"   (dropped {c['dropped']})" if c["dropped"] else ""))
        if c.get("unscoreable"):
            print(f"    UNSCOREABLE: {c['unscoreable']}\n")
            continue
        print(f"    reps {c['reps']}   windows/rep {c['windows_per_rep']}   "
              f"median window {c['median_window_s']:.3f}s   "
              f"pairs {n*(n-1)//2}   floor rho_bar >= {rho_floor(n):+.3f}")
        for k in ("window_mismatch", "underpowered"):
            if c.get(k):
                print(f"    ** {k.upper()}: {c[k]}")
        print(f"    {'series':<26} {'rho_raw':>8} {'rho_ctr':>8} "
              f"{'95% CI (centered)':>22} {'rho_2way':>9} {'95% CI (2way)':>19}"
              f" {'B(N,rho)':>9}")
        for key, name, _ in SERIES:
            e = c["series"][key]
            ci = ("        n/a" if e["ci95"][0] is None
                  else f"[{e['ci95'][0]:>+6.3f}, {e['ci95'][1]:>+6.3f}]")
            ci2 = ("     n/a" if e["ci95_2way"][0] is None
                   else f"[{e['ci95_2way'][0]:>+6.3f}, {e['ci95_2way'][1]:>+6.3f}]")
            b = e["B_2way"]
            bs = "      n/a" if b is None else f"{b:>9.3f}"
            print(f"    {name:<26} {fmt(e['rho_bar_raw'])} {fmt(e['rho_bar_centered'])}"
                  f" {ci:>22} {fmt(e['rho_bar_2way']):>9} {ci2:>19} {bs}")
            if e["below_floor_2way"]:
                print(f"      ** INTERNAL INCONSISTENCY: rho_bar "
                      f"{e['rho_bar_2way']:+.3f} is below {e['floor']:+.3f}, "
                      f"which positive-semi-definiteness of any correlation "
                      f"matrix forbids. This is a BUG IN THIS SCORER, not a "
                      f"measurement — every number on this line is void.")
        # The six pairs behind the mean, on the drain series.
        e = c["series"]["rate_lr"]
        if n > 2:
            print("    pairwise (delivered rate, two-way): " + "  ".join(
                f"{k}:{'n/a' if v is None else f'{v:+.3f}'}"
                for k, v in sorted(e["pairs_2way"].items())))
            if e.get("class_2way"):
                cl = e["class_2way"]
                print("    class means (two-way): " + "  ".join(
                    f"{k}:{'n/a' if cl[k] is None else f'{cl[k]:+.3f}'}"
                    for k in ("fast_fast", "slow_slow", "fast_slow")))
        for pk, lv in c["levels"].items():
            print(f"    {pk}: rate_lr mean {lv['rate_lr_mean']:>8.0f} sym/s "
                  f"sd {lv['rate_lr_sd']:>7.0f} (CV {100*lv['rate_lr_cv']:>5.1f}%)   "
                  f"rtprop mean {lv['rtprop_mean']:>6.2f} ms")
        print()


# ── the self-check: this file's N-path code, on the published N=2 result ──
#
# The c7/c8 numbers as PUBLISHED in goal-gate "Eppen's Condition at c8" §2,
# which were produced by `eppen_corr.py`. Reproducing them here is the only
# evidence that the generalization is a widening and not a rewrite.
SELFCHECK = {
    ("c7", "rate_lr"): {"centered": +0.048, "two_way": -0.814},
    ("c7", "acks"): {"centered": +0.259, "two_way": -0.818},
    ("c8", "rate_lr"): {"centered": +0.800, "two_way": +0.612},
    ("c8", "acks"): {"centered": +0.795, "two_way": +0.610},
}


def selfcheck(result):
    """Require this file's N-path estimators to reproduce the published N = 2
    numbers to +-0.001. Returns the number of failures."""
    print("--- SELFCHECK: N-path code vs the PUBLISHED N=2 result "
          "(goal-gate \"Eppen's Condition at c8\" §2, produced by eppen_corr.py)")
    print(f"    {'cell/series':<22} {'estimator':<10} {'published':>10} "
          f"{'this file':>10} {'delta':>8}")
    bad = 0
    for (cell, key), want in sorted(SELFCHECK.items()):
        c = result["cells"].get(cell)
        if not c or key not in c.get("series", {}):
            print(f"    {cell+'/'+key:<22} {'—':<10} {'—':>10} "
                  f"{'ABSENT':>10}   (cell not in these ledgers)")
            continue
        e = c["series"][key]
        for label, field, w in (("rep-ctr", "rho_bar_centered", want["centered"]),
                                ("two-way", "rho_bar_2way", want["two_way"])):
            got = e[field]
            d = float("nan") if got is None else got - w
            ok = got is not None and abs(d) <= 0.001
            bad += 0 if ok else 1
            print(f"    {cell+'/'+key:<22} {label:<10} {w:>+10.3f} "
                  f"{('n/a' if got is None else f'{got:+.3f}'):>10} "
                  f"{('n/a' if got is None else f'{d:+.4f}'):>8}"
                  f"{'' if ok else '   <== MISMATCH'}")
    print(f"\n    SELFCHECK: {'PASS' if bad == 0 else f'FAIL ({bad} mismatches)'}"
          f" — at N = 2 the mean of C(2,2)=1 pairwise correlations IS the pair,\n"
          f"    so this file's rho_bar must equal eppen_corr.py's rho exactly.\n")
    return bad


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("ledgers", nargs="+")
    ap.add_argument("--qdir", default=None, help="dir of *-q.txt qdisc captures")
    ap.add_argument("--window-us", type=int, default=C9_WINDOW_US,
                    help="the cadence the ledger was CAPTURED at, µs "
                         f"(default {C9_WINDOW_US}; the shipped default is "
                         f"{SHIPPED_WINDOW_US})")
    ap.add_argument("--selfcheck", action="store_true",
                    help="reproduce the published N=2 c7/c8 numbers and exit "
                         "non-zero on any mismatch")
    ap.add_argument("--json", action="store_true")
    args = ap.parse_args()

    recs = parse_lines(args.ledgers)
    if not recs:
        print("NO [ACKDIAG] LINES — instrument absent, nothing scored.",
              file=sys.stderr)
        return 2

    print("=" * 78)
    print("EPPEN'S CORRELATION CONDITION at N PATHS — the c9 quad scorer")
    print("=" * 78)
    print(f"instrument: [ACKDIAG] per-path window series (src/net/ackdiag.rs)")
    print(f"cadence:    {args.window_us} us declared"
          + ("  [THE SHIPPED 2 s DEFAULT — a quad is UNSCOREABLE here]"
             if args.window_us == SHIPPED_WINDOW_US else ""))
    print(f"benefit:    B(N, rho_bar) = 1 - sqrt((1 + (N-1)*rho_bar)/N)")
    print(f"floor:      rho_bar >= -1/(N-1)   = -1.000 at N=2, -0.333 at N=4\n")

    result = {"cells": {}, "window_us": args.window_us, "seed_audit": []}
    for cell in sorted({r["cell"] for r in recs}):
        result["cells"][cell] = score_cell(recs, cell, args.window_us)
    report(result)

    rc = 0
    if args.selfcheck:
        rc = 1 if selfcheck(result) else 0

    if args.qdir:
        aud = seed_audit(args.qdir)
        result["seed_audit"] = aud
        print("--- SEED AUDIT (per-leg netem seeds; see the HARNESS ERA note "
              "in lib.sh)")
        print(f"    {'capture':<30} {'legs':>5} {'distinct':>9} {'per-leg?':>9}"
              f"  seeds")
        for a in aud:
            print(f"    {a['capture']:<30} {a['n_legs']:>5} "
                  f"{a['distinct_seeds']:>9} {str(a['per_leg_seeds']):>9}"
                  f"  {','.join(str(s) for s in a['seeds'])}")
        shared = [a for a in aud if a["all_seeds_equal"] and a["symmetric_params"]]
        print(f"\n    IDENTICAL DEMAND REALIZATION (one seed AND symmetric "
              f"params): {len(shared)}/{len(aud)} captures.")
        print("    At a SYMMETRIC cell that is rho_loss = +1 BY CONSTRUCTION —\n"
              "    the pre-2026-08-19 harness era. A post-repair quad capture\n"
              "    must read distinct=4 / per-leg=True here.\n")

    if args.json:
        print(json.dumps(result, indent=2, default=str))
    return rc


if __name__ == "__main__":
    sys.exit(main())
