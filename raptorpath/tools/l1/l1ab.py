"""The A/B ship-battery scoring helpers shared by the `thread*` harnesses
(`threadq1/q2/d9/r1/r2/sk_parse.py`, and `rdiag` also by `p0_parse.py`): row
selection, the median/min-max `reading`, the cell blockers, the binary-sha
check. Moved here verbatim from those files, which re-export every name, so
`threadsk_parse.reading` etc. still resolve.

It lives apart from `l1common.py` because `per_byte` reads
`stage3_parse.CELL_SPEC` and `stage3_parse` imports `l1common` (a cycle).
"""
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import l1common as lc  # noqa: E402
import stage3_parse as sp  # noqa: E402

REL_FLOOR = 0.05          # RTprop floor / [LAG] p99 or E: max(5 %, MAIN half-range / median) (§11-§13)
MIN_LIVE = 3              # live rows per (cell, arm)
WITNESS_FAIL_LIMIT = 2    # failed rows per (arm, cell) that void the cell
RDIAG_MIN_MSGS = 1000     # msgs/s: an [RDIAG] line inside the transfer

_RDIAG = re.compile(r"\[RDIAG\] busy=(-?[0-9.]+)% msgs=(\d+)/s q_avg=([0-9.]+) q_max=(\d+)")


def bin_check(rows, shas):
    for r in rows:
        want = shas.get(r.get("bin"))
        if want and r.get("bin_sha") and r["bin_sha"] != want and r["status"] == "LIVE":
            r["status"] = "CONTAMINATED"
            r["problems"].append("bin-sha-mismatch")
    return rows


def sel(live, cell, arm, seed=None):
    return [r for r in live if r["cell"] == cell and r["arm"] == arm
            and (seed is None or r["seed"] == seed)]


def vals(rows, key):
    return [r[key] for r in rows if r["dnf"] is False and r.get(key) is not None]


def per_byte(rows, key):
    out = []
    for r in rows:
        if r["dnf"] is False and r.get(key) is not None:
            b = sp.CELL_SPEC[r["cell"]][3] if len(sp.CELL_SPEC[r["cell"]]) > 3 else None
            out.append(r[key] / (b / 1e9) if b else r[key])
    return out


def reading(xv, rv, rel, higher_is_better, name="ARM"):
    """The median clause with the min-max rule (arm against MAIN)."""
    if not rv:
        return "VACUOUS", "reference has no value"
    if not xv:
        return "WORSE", f"{name} has no value"
    xm, rm = lc.med(xv), lc.med(rv)
    lo, hi = rm * (1 - rel), rm * (1 + rel)
    det = (f"{name}={xm:.4g} [{min(xv):.4g}-{max(xv):.4g}] MAIN={rm:.4g} [{min(rv):.4g}-{max(rv):.4g}] "
           f"({100 * (xm / rm - 1):+.1f}%, rel={rel:.3f})")
    if higher_is_better:
        worse_med, better_med = xm < lo, xm > hi
        disjoint_worse, disjoint_better = max(xv) < min(rv), min(xv) > max(rv)
    else:
        worse_med, better_med = xm > hi, xm < lo
        disjoint_worse, disjoint_better = min(xv) > max(rv), max(xv) < min(rv)
    if worse_med:
        return ("WORSE" if disjoint_worse else "TREND-WORSE"), det
    if better_med:
        return ("BETTER" if disjoint_better else "TREND-BETTER"), det
    return "WITHIN", det


def spread_rel(rv):
    if not rv:
        return REL_FLOOR
    m = lc.med(rv)
    return max(REL_FLOOR, 0.5 * (max(rv) - min(rv)) / m if m else 0.0)


def witness_fails(rows, arm, cell):
    return sum(1 for r in rows if r["arm"] == arm and r["cell"] == cell
               and r["status"] in ("CONTAMINATED", "WITNESS-FAIL"))


def blockers(rows, live, cell, arm, aborted):
    why = []
    for a in ("MAIN", arm):
        nl = len(sel(live, cell, a))
        if nl < MIN_LIVE:
            why.append(f"{a} live={nl}<{MIN_LIVE}")
        wf = witness_fails(rows, a, cell)
        if wf >= WITNESS_FAIL_LIMIT:
            why.append(f"{a} witness-failed={wf}")
    if aborted:
        why.append("abort")
    return why


def mmm(v, nd=2):
    m = lc.med(v)
    return "-" if m is None else f"{m:.{nd}f} [{min(v):.{nd}f}-{max(v):.{nd}f}] n={len(v)}"


def rdiag(lines):
    """The in-transfer `[RDIAG]` lines -> (busy_med, busy_max, q_avg_med, n)."""
    busy, qs = [], []
    for ln in lines or []:
        m = _RDIAG.search(ln)
        if m and int(m.group(2)) >= RDIAG_MIN_MSGS:
            busy.append(float(m.group(1)))
            qs.append(float(m.group(3)))
    if not busy:
        return None, None, None, 0
    return lc.med(busy), max(busy), lc.med(qs), len(busy)
