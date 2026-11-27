"""U's own release cost and its exact offline optimum U* (skip-rule-efficiency.md 7.3).

Per hole: t = detect time, r = end of the hold = min(R fill, t_F + H pass, end of run), ind = 1{R < t_F + H} (a release
before the fill would have bought disorder).  cost(Z) = kappa * |union of [t_s, r_s) over held Z| + omega * sum_{s not in Z} ind_s,
kappa = delta / d_end.  The sweep is exact; the TU LP and brute force are its cross-checks (P4e).
"""
import itertools
import math

import numpy as np


def union_len(ivs):
    tot, cur_s, cur_e = 0.0, None, None
    for a, b in sorted(ivs):
        if b <= a:
            continue
        if cur_e is None or a > cur_e:
            if cur_e is not None:
                tot += cur_e - cur_s
            cur_s, cur_e = a, b
        elif b > cur_e:
            cur_e = b
    if cur_e is not None:
        tot += cur_e - cur_s
    return tot


def sweep_min(t, r, ind, kappa, omega):
    """Exact optimum by the sweep of 7.3: state = right end e of the held union."""
    order = sorted(range(len(t)), key=lambda i: (t[i], i))
    states = [(-math.inf, 0.0)]
    for i in order:
        ti, ri, wi = t[i], r[i], omega * ind[i]
        new = {}
        for e, c in states:
            e0 = max(e, ti)                              # states with e <= t_s merge
            # hold
            h_e = max(e0, ri)
            h_c = c + kappa * max(0.0, ri - e0)
            if h_e not in new or h_c < new[h_e]:
                new[h_e] = h_c
            # release
            if e0 not in new or c + wi < new[e0]:
                new[e0] = c + wi
        items = sorted(new.items())                      # ascending e
        keep = []
        best = math.inf                                   # larger e at no higher cost dominates
        for e, c in reversed(items):
            if c < best:
                keep.append((e, c))
                best = c
        keep.reverse()
        out = []
        best = math.inf                                   # smaller e at cost lower by >= kappa*(e_a - e_b) dominates
        for e, c in keep:
            cp = c - kappa * e
            if cp < best:
                out.append((e, c))
                best = cp
        states = out
    return min(c for e, c in states)


def brute_min(t, r, ind, kappa, omega):
    n = len(t)
    best = math.inf
    for mask in range(1 << n):
        ivs = [(t[i], r[i]) for i in range(n) if mask >> i & 1]
        c = kappa * union_len(ivs) + omega * sum(ind[i] for i in range(n) if not mask >> i & 1)
        best = min(best, c)
    return best


def lp_min(t, r, ind, kappa, omega):
    """Totally unimodular LP of 6.3(b).  Returns (value, z integral?)."""
    from scipy.optimize import linprog
    n = len(t)
    pts = sorted({x for i in range(n) for x in (t[i], r[i])})
    segs = [(pts[k], pts[k + 1]) for k in range(len(pts) - 1)]
    m = len(segs)
    # variables: z_0..z_{n-1}, u_0..u_{m-1}; minimise kappa*sum l_k u_k - omega*sum ind_s z_s  (+ omega*sum ind)
    cost = np.concatenate([-omega * np.asarray(ind, float), kappa * np.asarray([b - a for a, b in segs])])
    rows, cols, vals = [], [], []
    nr = 0
    for i in range(n):
        for k, (a, b) in enumerate(segs):
            if a >= t[i] and b <= r[i]:
                rows += [nr, nr]
                cols += [i, n + k]
                vals += [1.0, -1.0]
                nr += 1
    if nr:
        from scipy.sparse import coo_matrix
        A = coo_matrix((vals, (rows, cols)), shape=(nr, n + m)).tocsr()
        res = linprog(cost, A_ub=A, b_ub=np.zeros(nr), bounds=(0, 1), method="highs")
    else:
        res = linprog(cost, bounds=(0, 1), method="highs")
    x = res.x
    integral = bool(np.all(np.minimum(np.abs(x), np.abs(1 - x)) < 1e-7))
    return res.fun + omega * float(np.sum(ind)), integral


def clusters(t, r):
    """Connected components of the union of the hole intervals; returns a list of index lists."""
    order = sorted(range(len(t)), key=lambda i: (t[i], i))
    out, cur, end = [], [], None
    for i in order:
        if cur and t[i] < end:
            cur.append(i)
            end = max(end, r[i])
        else:
            if cur:
                out.append(cur)
            cur, end = [i], r[i]
    if cur:
        out.append(cur)
    return out


def u_costs(sim, rows):
    """U's own cost and U* for a finished run.  rows: (seq, t_detect, R, t_pass, t_release)."""
    c = sim.cfg
    h_end = sim.stop_at if sim.stop_at is not None else sim.t_end
    d_end = sim.d_now()
    if d_end is None or not rows:
        return dict(u_cost=None)
    kappa = c.delta / d_end
    t = [x[1] for x in rows]
    R = [x[2] for x in rows]
    P = [x[3] for x in rows]
    T = [x[4] for x in rows]
    r = [min(R[i], P[i], h_end) for i in range(len(rows))]
    ind = [1 if (R[i] < P[i] and R[i] <= h_end) else 0 for i in range(len(rows))]
    ends = [min(r[i], T[i]) for i in range(len(rows))]
    rent = kappa * union_len(list(zip(t, ends)))
    buy = c.omega * sum(1 for i in range(len(rows)) if T[i] < math.inf and ind[i])
    star = sweep_min(t, r, ind, kappa, c.omega)
    out = dict(u_kappa=kappa, u_d_end=d_end, u_rent=rent, u_buy=buy, u_cost=rent + buy, u_star=star,
               u_ratio=(rent + buy) / star if star > 0 else (None if rent + buy == 0 else math.inf),
               u_holes=len(rows), u_ind=int(sum(ind)), u_ninf=len(rows) - int(sum(ind)))
    # isolated holes: clusters of size one (the 2x bound is proven there)
    iso_u = iso_s = 0.0
    n_iso = n_inf = 0
    for cl in clusters(t, r):
        if len(cl) != 1:
            continue
        i = cl[0]
        n_iso += 1
        n_inf += 1 - ind[i]
        iso_u += kappa * max(0.0, ends[i] - t[i]) + (c.omega if (T[i] < math.inf and ind[i]) else 0.0)
        iso_s += min(kappa * max(0.0, r[i] - t[i]), c.omega * ind[i])
    out.update(u_iso_n=n_iso, u_iso_ninf=n_inf, u_iso_cost=iso_u, u_iso_star=iso_s,
               u_iso_slack=(iso_u - (2 * iso_s + c.omega * n_inf)))
    return out
