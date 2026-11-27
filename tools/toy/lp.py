"""Exact offline lower bound for passing rules (spec 6.3a), plus a brute force for tiny cases.

Instance (per sample path, from the S0 trace):
  a[j]   arrival time of a received non-hole packet, nan for holes
  X[s]   fill time of hole s under S0 (finite; use t_end + 1 for never-filled), nan for non-holes
  p[s]   earliest pass instant t_detect(s) + H, nan for non-holes
  sent_t sorted send times (S_src(t) = number of sends at or before t)

  minimise  sum over non-hole j of (y_j - a_j)
  s.t.      y_j >= a_j, y_j >= y_{j-1}, y_s >= X_s - (X_s - p_s)^+ x_s,
            sum_{s: p_s <= t_k} c_s x_s <= floor((1-rho) S_src(t_k)) at every pass instant, 0 <= x <= 1
"""
import itertools

import numpy as np
from scipy.optimize import linprog, milp, LinearConstraint, Bounds
from scipy.sparse import coo_matrix


def _budget(p_sorted, sent_t, rho):
    cnt = np.searchsorted(np.asarray(sent_t), p_sorted, side="right")
    return np.floor((1 - rho) * cnt + 1e-9)


def hol_s0(a, X):
    n = len(a)
    hole = np.isnan(a)
    base = np.where(hole, X, a)
    y0 = np.maximum.accumulate(base)
    return float(np.sum((y0 - a)[~hole]))


def solve(a, X, p, sent_t, rho, integer=False, method="highs", time_limit=None):
    """Return (hol_lp, hol_s0, x) where hol_* are sums over non-hole packets."""
    a = np.asarray(a, float)
    X = np.asarray(X, float)
    p = np.asarray(p, float)
    n = len(a)
    hole = np.isnan(a)
    base = np.where(hole, X, a)
    y0 = np.maximum.accumulate(base)
    nonhole_var = (~hole) & (y0 > a + 1e-12)
    var = hole | nonhole_var
    vidx = -np.ones(n, dtype=np.int64)
    vi = np.nonzero(var)[0]
    vidx[vi] = np.arange(len(vi))
    ny = len(vi)
    c_s = hole & (X > p + 1e-12)
    hs = np.nonzero(c_s)[0]
    hs = hs[np.argsort(p[hs], kind="stable")]
    nx = len(hs)
    xidx = {int(s): ny + k for k, s in enumerate(hs)}
    nz = nx
    ntot = ny + nx + nz
    lb = np.zeros(ntot)
    ub = np.zeros(ntot)
    # y bounds
    for k, j in enumerate(vi):
        if hole[j]:
            lb[k] = min(X[j], p[j])
        else:
            lb[k] = a[j]
        ub[k] = y0[j]
    # chain from fixed predecessors raises lower bounds
    rows, cols, vals, rhs = [], [], [], []
    r = 0
    for k, j in enumerate(vi):
        if j > 0:
            q = vidx[j - 1]
            if q >= 0:
                rows += [r, r]
                cols += [q, k]
                vals += [1.0, -1.0]
                rhs.append(0.0)
                r += 1
            else:
                lb[k] = max(lb[k], a[j - 1] if not hole[j - 1] else base[j - 1])
        if hole[j]:
            if c_s[j]:
                # -y - (X-p) x <= -X
                rows += [r, r]
                cols += [k, xidx[int(j)]]
                vals += [-1.0, -(X[j] - p[j])]
                rhs.append(-X[j])
                r += 1
            else:
                lb[k] = max(lb[k], X[j])
        ub[k] = max(ub[k], lb[k])
    ub[ny:ny + nx] = 1.0
    if nx:
        B = _budget(p[hs], sent_t, rho)
        ub[ny + nx:] = B
        lb[ny + nx:] = 0.0
    A_ub = coo_matrix((vals, (rows, cols)), shape=(r, ntot)).tocsr() if r else None
    b_ub = np.asarray(rhs) if r else None
    # z_k - z_{k-1} - x_k = 0
    er, ec, ev = [], [], []
    for k in range(nx):
        er += [k, k]
        ec += [ny + nx + k, ny + k]
        ev += [1.0, -1.0]
        if k > 0:
            er.append(k)
            ec.append(ny + nx + k - 1)
            ev.append(-1.0)
    A_eq = coo_matrix((ev, (er, ec)), shape=(nx, ntot)).tocsr() if nx else None
    b_eq = np.zeros(nx) if nx else None
    cobj = np.zeros(ntot)
    nh = np.nonzero((~hole)[vi])[0]
    cobj[nh] = 1.0
    const = float(np.sum(a[vi[nh]]))
    s0 = float(np.sum((y0 - a)[~hole]))
    if ntot == 0:
        return 0.0, s0, {}
    if integer and nx:
        cons = []
        if A_ub is not None:
            cons.append(LinearConstraint(A_ub, -np.inf, b_ub))
        cons.append(LinearConstraint(A_eq, b_eq, b_eq))
        integ = np.zeros(ntot)
        integ[ny:ny + nx] = 1
        res = milp(cobj, constraints=cons, bounds=Bounds(lb, ub), integrality=integ,
                   options={"time_limit": time_limit} if time_limit else None)
        ok = res.status == 0 or (res.status == 1 and res.x is not None)   # time limit with an incumbent: upper bound
    else:
        res = linprog(cobj, A_ub=A_ub, b_ub=b_ub, A_eq=A_eq, b_eq=b_eq, bounds=np.column_stack([lb, ub]), method=method)
        ok = res.status == 0
    if not ok:
        raise RuntimeError("LP failed: %s" % res.message)
    x = {int(s): float(res.x[ny + k]) for k, s in enumerate(hs)}
    return float(res.fun) - const, s0, x


def brute(a, X, p, sent_t, rho):
    """Exhaustive minimum over x in {0,1}^holes (small cases only)."""
    a = np.asarray(a, float)
    X = np.asarray(X, float)
    p = np.asarray(p, float)
    hole = np.isnan(a)
    c_s = hole & (X > p + 1e-12)
    hs = np.nonzero(c_s)[0]
    hs = hs[np.argsort(p[hs], kind="stable")]
    B = _budget(p[hs], sent_t, rho) if len(hs) else []
    best = None
    for bits in itertools.product((0, 1), repeat=len(hs)):
        cum = np.cumsum(bits) if len(hs) else []
        if any(cum[k] > B[k] for k in range(len(hs))):
            continue
        base = np.where(hole, X, a).astype(float)
        for b, s in zip(bits, hs):
            if b:
                base[s] = p[s]
        y = np.maximum.accumulate(base)
        tot = float(np.sum((y - a)[~hole]))
        if best is None or tot < best:
            best = tot
    return best


def instance_from_run(res, t_end, H=None):
    """Build the LP instance from a want_trace summary of an S0 run."""
    n = res["n_src"]
    arr, wh, got = res["_arr"], res["_wh"], res["_got"]
    tdet, tfill = res["_tdet"], res["_tfill"]
    H = res["_H"] if H is None else H
    a = np.full(n, np.nan)
    X = np.full(n, np.nan)
    p = np.full(n, np.nan)
    for j in range(n):
        if wh[j]:
            X[j] = tfill.get(j, t_end + 1.0)
            p[j] = tdet[j] + H
        elif got[j]:
            a[j] = arr[j]
        else:
            # never received and never a hole (tail beyond highest arrival): treat as late hole
            X[j] = t_end + 1.0
            p[j] = res["_sent_t"][j] + H
    return a, X, p, np.asarray(res["_sent_t"][:n])
