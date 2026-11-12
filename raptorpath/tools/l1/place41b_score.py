#!/usr/bin/env python3
"""The §41b guard: an A/A-calibrated, one-sided, pooled-relabelling statistic (docs/status.md "## 41b").

    place41b_score.py score <ledger>...     the real scoring: arms BASE (12 per cell), MAIN (6), WIRE (6, reported only)
    place41b_score.py dry <ledger>...       the same code on a ledger with fewer BASE rows (§41's: BASE 6, MAIN 6 per cell)
    options: --method relabel-max (the registered 41b statistic, default) | rank-minp (stratified rank sums, min-p; status 47 candidate)
             --guard   four cells (c1s c1d c2 c8), CAND against BASE, no WIRE arm

Fixed by the spec, implemented exactly:
  * per cell the rows of BASE and the tested arm are pooled; one relabelling picks, per seed, as many rows as the tested arm
    has for that seed (3 + 3 for the real design) as "tested"; the rest are the reference;
  * clause value x_c = worse-signed (med(sel) - med(ref)) / med(ref): lower goodput, higher client / server CPU per GB, higher
    lag_E_cli / lag_E_srv, higher rtp floor per leg are worse;
  * z_c = x_c / sd_c, sd_c the (sample) sd of x_c over N_PERM = 10 000 relabellings, RNG seed 4141 (random.Random, one stream:
    relabelling i draws for cell c1s then c1d, seeds in sorted order, random.sample of the index range);
  * T = the max of z_c over every clause of both cells; p = share of relabellings with T >= T(true); PASS iff p > 0.05;
  * the resolution table (BASE alone, printed first): per clause the q95 of |x_c| over the "half of each seed" splits of the BASE
    rows (3 + 3 vs 6 for 12 BASE rows; the rank is ceil(0.95 n) of the sorted values), beside the §21 tolerance.
Rows: LIVE and not DNF; a row lacking a clause value is skipped for that clause only.
"""
import itertools
import math
import os
import random
import statistics
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import l1common as lc  # noqa: E402
import stage3_parse as sp  # noqa: E402
from l1ab import spread_rel  # noqa: E402

N_PERM = 10000
RNG_SEED = 4141
P_PASS = 0.05
SLOW_FRAC = 0.5                 # a completed row below this fraction of the pooled cell median goodput is listed (reported only)
CELLS = [("c1s-400", "def"), ("c1d-400", "def")]
# Status 47: the guard on the merge candidate runs at four cells (BASE n = 12, CAND n = 6 each), the same statistic over all their clauses.
CELLS_GUARD = [("c1s-400", "def"), ("c1d-400", "def"), ("c2-100", "def"), ("c8-100", "def")]


def clause_defs(cell):
    """[(name, row key, worse sign)]: sign +1 = higher is worse."""
    d = [("goodput", "mbps", -1), ("cpu_cli/GB", "cpu_cli", 1), ("cpu_srv/GB", "cpu_srv", 1),
         ("lag_E_cli", "cli_lag_E", 1), ("lag_E_srv", "srv_lag_E", 1)]
    d += [(f"rtp_floor_p{i}", f"rtp_floor_p{i}", 1) for i in range(sp.N_LEGS[cell])]
    return d


def value(r, cell, key):
    if r.get("dnf") or r.get(key) is None:
        return None
    if key in ("cpu_cli", "cpu_srv"):
        b = r.get("bytes") or sp.CELL_SPEC[cell][3]       # plan 48 sizes its cells itself and records the row's bytes
        return r[key] / (b / 1e9)
    return r[key]


def med(v):
    v = [x for x in v if x is not None]
    return lc.med(v) if v else None


def x_of(vals, sel, ref, sign):
    ms, mr = med([vals[i] for i in sel]), med([vals[i] for i in ref])
    if ms is None or not mr:
        return None
    return sign * (ms - mr) / mr


def q95(v):
    v = sorted(v)
    return v[max(0, math.ceil(0.95 * len(v)) - 1)] if v else None


def live(rows, cell, arm):
    return [r for r in rows if r["status"] == "LIVE" and r["cell"] == cell and r["store"] == "def" and r["arm"] == arm]


# A CRASH (rc != 0 -> VOID-RC; no summary -> NO_DATA) is never replaced and, like a DNF, is the worst rank on every clause of its cell in the rank
# statistic (status 47). Rows voided by a witness (WITNESS-FAIL, CONTAMINATED, VOID-COTENANT, VOID-ONDISK) are replaceable and are not scored.
CRASH_STATUSES = ("VOID-RC", "NO_DATA")


def crash(rows, cell, arm):
    return [r for r in rows if r["status"] in CRASH_STATUSES and r["cell"] == cell and r["store"] == "def" and r["arm"] == arm]


class Pool:
    """One cell's pooled rows (reference arm first, tested arm after) and the true selection."""

    def __init__(self, rows, cell, ref_arm, test_arm, exclude=(), with_crash=False):
        self.cell = cell
        self.ref_rows, self.test_rows = live(rows, cell, ref_arm), live(rows, cell, test_arm)
        if with_crash:                          # crash rows fill their slots (and are the worst rank); only the rank statistic asks for them
            self.ref_rows, self.test_rows = self.ref_rows + crash(rows, cell, ref_arm), self.test_rows + crash(rows, cell, test_arm)
        self.rows = self.ref_rows + self.test_rows
        self.true = list(range(len(self.ref_rows), len(self.rows)))
        # exclude: clause names (every cell) or (cell, clause) pairs (one cell: status 48b's p_rest drops only c2's and c8's goodput)
        self.defs = [d for d in clause_defs(cell) if d[0] not in exclude and (cell, d[0]) not in exclude]
        # COLLAPSE (status 47) is harness 43's definition (place43_parse.py: "COLLAPSE is the DNF row only", amendment 2): a DNF row is the WORST
        # rank on every clause of its cell in the rank statistic. A completed row far below the cell's goodput is NOT forced: it ranks by
        # its own values and is listed as a slow row (goodput < SLOW_FRAC x the median of the cell's POOLED rows, BASE and CAND together,
        # DNF excluded: label-free, never BASE-only).
        mb = med([r["mbps"] for r in self.rows if not r.get("dnf") and r.get("mbps") is not None])
        self.bad_why = ["DNF" if r.get("dnf") else (f"CRASH({r['status']})" if r["status"] in CRASH_STATUSES else None) for r in self.rows]
        self.bad = [w is not None for w in self.bad_why]
        self.slow = [bool(mb and not r.get("dnf") and r.get("mbps") is not None and r["mbps"] < SLOW_FRAC * mb) for r in self.rows]
        self.vals = {n: [value(r, cell, k) for r in self.rows] for n, k, _ in self.defs}
        self.seeds = sorted({r["seed"] for r in self.rows})
        self.groups = {s: [i for i, r in enumerate(self.rows) if r["seed"] == s] for s in self.seeds}
        self.k = {s: sum(1 for i in self.true if self.rows[i]["seed"] == s) for s in self.seeds}

    def x_all(self, sel):
        ss = set(sel)
        ref = [i for i in range(len(self.rows)) if i not in ss]
        return {n: x_of(self.vals[n], sel, ref, sg) for n, _, sg in self.defs}

    def draw(self, rng):
        sel = []
        for s in self.seeds:
            sel += rng.sample(self.groups[s], self.k[s])
        return sel


def relabel(pools, n_perm=N_PERM, seed=RNG_SEED):
    """-> (x_true {(cell, clause): x}, xs {(cell, clause): [x per relabelling]})"""
    rng = random.Random(seed)
    xs = {(p.cell, n): [] for p in pools for n, _, _ in p.defs}
    for _ in range(n_perm):
        for p in pools:
            for n, x in p.x_all(p.draw(rng)).items():
                xs[(p.cell, n)].append(x)
    xt = {(p.cell, n): x for p in pools for n, x in p.x_all(p.true).items()}
    return xt, xs


def statistic(rows, ref_arm, test_arm, n_perm=N_PERM, seed=RNG_SEED, cells=None):
    pools = [Pool(rows, cell, ref_arm, test_arm) for cell, _ in (cells or CELLS)]
    xt, xs = relabel(pools, n_perm, seed)
    sd = {}
    for k, v in xs.items():
        v2 = [x for x in v if x is not None]
        sd[k] = statistics.stdev(v2) if len(v2) > 1 else None
    keys = [k for k in xs if sd[k]]
    z_true = {k: (xt[k] / sd[k]) if xt[k] is not None else None for k in keys}
    T_true = max(z for z in z_true.values() if z is not None)
    Ts, arg = [], {}
    for i in range(n_perm):
        best, bk = None, None
        for k in keys:
            x = xs[k][i]
            if x is None:
                continue
            z = x / sd[k]
            if best is None or z > best:
                best, bk = z, k
        Ts.append(best)
        arg[bk] = arg.get(bk, 0) + 1
    p = sum(1 for t in Ts if t is not None and t >= T_true) / n_perm
    arg_true = max((k for k in keys if z_true[k] is not None), key=lambda k: z_true[k])
    return {"pools": pools, "x_true": xt, "sd": sd, "z_true": z_true, "T_true": T_true, "arg_true": arg_true, "p": p,
            "argmax_share": {k: v / n_perm for k, v in sorted(arg.items(), key=lambda kv: -kv[1])}, "n_perm": n_perm}


def _midranks(vals):
    """Midranks (1-based, ties averaged) of a list; None -> the neutral mean rank."""
    idx = sorted((i for i, v in enumerate(vals) if v is not None), key=lambda i: vals[i])
    out = [None] * len(vals)
    j = 0
    while j < len(idx):
        k = j
        while k + 1 < len(idx) and vals[idx[k + 1]] == vals[idx[j]]:
            k += 1
        for t in range(j, k + 1):
            out[idx[t]] = (j + k) / 2 + 1
        j = k + 1
    neutral = (len(vals) + 1) / 2
    return [neutral if v is None else v for v in out]


def rank_scores(pool):
    """Per clause, per pooled row: the WITHIN-SEED worse-side midrank (higher = worse: a higher CPU / lag / rtp floor, a lower goodput).
    A DNF or collapsed row (pool.bad) is worse than every other row on EVERY clause (value +inf; several such rows tie at the top);
    only an absent instrument value on an otherwise completed row is neutral (the stratum's mean rank), and those fills are counted
    (pool.neutral[clause])."""
    sc = {}
    pool.neutral = {}
    inf = float("inf")
    for n, _, sg in pool.defs:
        v = [None] * len(pool.rows)
        pool.neutral[n] = sum(1 for i, b in enumerate(pool.bad) if not b and pool.vals[n][i] is None)
        for s_, idxs in pool.groups.items():
            ranks = _midranks([inf if pool.bad[i] else (None if pool.vals[n][i] is None else sg * pool.vals[n][i]) for i in idxs])
            for i, r in zip(idxs, ranks):
                v[i] = r
        sc[n] = v
    return sc


def statistic_rank(rows, ref_arm, test_arm, n_perm=N_PERM, seed=RNG_SEED, cells=None, exclude=(), design=None):
    """The stratified rank-sum min-p guard (status 47 candidate). Per clause: the worse-side rank sum of the tested rows, ranks taken
    within seed. The clause statistics share the SAME joint relabellings as `statistic` (same RNG stream, same draws); each clause's
    permutation p is the share of labellings (the draws plus the true one) with a rank sum >= the labelling's own (ties extreme); the
    labelling's score is the MIN of its clause p-values over every clause of every cell; the gate p is the share of labellings whose
    min-p <= the true min-p. -> dict(p, T_true (the true min-p), p_true {(cell, clause): p}, S_true, argmin, n_perm)"""
    import bisect
    pools = [Pool(rows, cell, ref_arm, test_arm, exclude, with_crash=True) for cell, _ in (cells or CELLS)]
    sc = {p.cell: rank_scores(p) for p in pools}
    keys = [(p.cell, n) for p in pools for n, _, _ in p.defs]
    S = {k: [] for k in keys}
    rng = random.Random(seed)
    for _ in range(n_perm):
        for p in pools:
            sel = p.draw(rng)
            for n, _, _ in p.defs:
                v = sc[p.cell][n]
                S[(p.cell, n)].append(sum(v[i] for i in sel))
    S_true = {}
    for p in pools:
        for n, _, _ in p.defs:
            v = sc[p.cell][n]
            S_true[(p.cell, n)] = sum(v[i] for i in p.true)
    N1 = n_perm + 1
    pk = {}                                            # per clause: value -> p (share of the N+1 labellings with S >= value)
    for k in keys:
        allv = sorted(S[k] + [S_true[k]])
        pk[k] = {v: (N1 - bisect.bisect_left(allv, v)) / N1 for v in set(allv)}
    p_true = {k: pk[k][S_true[k]] for k in keys}
    T_true = min(p_true.values())
    hits = 1                                           # the true labelling itself
    for i in range(n_perm):
        if min(pk[k][S[k][i]] for k in keys) <= T_true + 1e-12:
            hits += 1
    arg = min(keys, key=lambda k: p_true[k])
    incomplete = []
    for cell, (nb, nc) in (design or {}).items():
        pl = next((p for p in pools if p.cell == cell), None)
        for arm, want, got in ((ref_arm, nb, len(pl.ref_rows) if pl else 0), (test_arm, nc, len(pl.test_rows) if pl else 0)):
            if got < want:
                incomplete.append((cell, arm, want, got))
    return {"pools": pools, "p": hits / N1, "T_true": T_true, "p_true": p_true, "S_true": S_true, "argmin": arg, "n_perm": n_perm,
            "S_null_mean": {k: sum(S[k]) / n_perm for k in keys},
            "neutral": {(p.cell, n): p.neutral[n] for p in pools for n in p.neutral},
            "bad": {p.cell: sum(p.bad) for p in pools},
            "bad_rows": [(p.cell, r["arm"], r["seed"], r["rep"], p.bad_why[i]) for p in pools for i, r in enumerate(p.rows) if p.bad[i]],
            "slow_rows": [(p.cell, r["arm"], r["seed"], r["rep"], r["mbps"]) for p in pools for i, r in enumerate(p.rows) if p.slow[i]],
            "incomplete": incomplete}


def resolution(rows, arm="BASE", cells=None):
    """BASE alone: per cell and clause the q95 of |x_c| over the half-per-seed splits, beside the §21 tolerance."""
    from place38_parse import S3_REL
    out = []
    for cell, _ in (cells or CELLS):
        base = live(rows, cell, arm)
        defs = clause_defs(cell)
        vals = {n: [value(r, cell, k) for r in base] for n, k, _ in defs}
        seeds = sorted({r["seed"] for r in base})
        groups = {s: [i for i, r in enumerate(base) if r["seed"] == s] for s in seeds}
        splits = [sum(c, ()) for c in itertools.product(*[list(itertools.combinations(groups[s], len(groups[s]) // 2)) for s in seeds])]
        rel = S3_REL[cell]
        for n, k, sg in defs:
            xv = []
            for sel in splits:
                ref = [i for i in range(len(base)) if i not in set(sel)]
                x = x_of(vals[n], sel, ref, sg)
                if x is not None:
                    xv.append(abs(x))
            allv = [v for v in vals[n] if v is not None]
            tol = rel["gp"] if n == "goodput" else rel["cpu"] if n.startswith("cpu") else spread_rel(allv)
            out.append({"cell": cell, "clause": n, "q95": q95(xv), "tol": tol, "splits": len(xv), "n_base": len(base)})
    return out


def tail_lines(rows, out, cells=None, arms=("BASE", "MAIN", "WIRE")):
    """Reported only: per cell and arm the lowest LIVE row's goodput against the cell median (the median of all live rows at the
    cell, every arm pooled), and how many of the arm's rows lie below 0.5 x that median."""
    for cell, _ in (cells or CELLS):
        pool = [r for a in arms for r in live(rows, cell, a) if r.get("mbps") is not None and not r.get("dnf")]
        m = med([r["mbps"] for r in pool])
        if not m:
            continue
        for arm in arms:
            rs = [r for r in pool if r["arm"] == arm]
            if not rs:
                continue
            lo = min(rs, key=lambda r: r["mbps"])
            n_lo = sum(1 for r in rs if r["mbps"] < 0.5 * m)
            out(f"  TAIL {cell} {arm:<4} lowest {lo['mbps']:.1f} Mbit/s (s{lo['seed']} r{lo['rep']}) = {lo['mbps'] / m:.3f} x cell median {m:.1f}; "
                f"rows below 0.5 x median: {n_lo} of {len(rs)}")


def report(rows, out=print, dry=False, cells=None, test_arm="MAIN", with_wire=True, title="§41b GUARD", gate_name="GATE-41b",
           subject="the gate-off binary", method="relabel-max", exclude=(), design=None, no_verdict_reason=None):
    """The registered guard. Defaults reproduce status 41b (BASE against MAIN, WIRE reported, c1s and c1d); the merge-candidate guard
    (status 47) passes cells=CELLS_GUARD, test_arm="CAND", with_wire=False."""
    cells = cells or CELLS
    ta = test_arm
    arms_all = ("BASE", ta, "WIRE") if with_wire else ("BASE", ta)
    ncell = {2: "both", 3: "all three", 4: "all four"}.get(len(cells), str(len(cells)))
    out(f"{title} ({'DRY RUN on a ledger with fewer BASE rows' if dry else 'real scoring'}): N_PERM={N_PERM} RNG seed {RNG_SEED}, one-sided, "
        f"max over every clause of {ncell} cells, PASS iff p > {P_PASS}")
    cnt = {(c, a): len(live(rows, c, a)) for c, _ in cells for a in arms_all}
    out("LIVE ROWS " + "  ".join(f"{c} {a}={cnt[(c, a)]}" for c, _ in cells for a in arms_all))
    out("")
    out(f"RESOLUTION (computed from BASE rows alone, before {ta} is read): per clause the q95 of |x_c| over the half-per-seed splits of BASE, beside the §21 tolerance")
    for r in resolution(rows, cells=cells):
        q = r["q95"]
        out(f"  {r['cell']} {r['clause']:<14} q95|x|={('%.4f' % q) if q is not None else '-'} (splits={r['splits']}, nBASE={r['n_base']})  §21 tol={r['tol']:.4f}  "
            + ("" if q is None else ("q95 > tol: the guard cannot see this clause below q95" if q > r["tol"] else "q95 <= tol")))
    out("")
    if method == "rank-minp":
        res = statistic_rank(rows, "BASE", ta, cells=cells, exclude=exclude, design=design)
        pools = res["pools"]
        out(f"{ta} against BASE (stratified rank sums, min-p over the same joint relabellings)")
        for p in pools:
            out(f"  {p.cell}: pool {len(p.ref_rows)} BASE + {len(p.test_rows)} {ta}; relabelled {ta} rows per seed " + " ".join(f"s{s}:{p.k[s]}" for s in p.seeds))
        out(f"  clause: worse-signed relative median difference x, rank sum S (true / null mean), clause permutation p  (positive x = {ta} worse)")
        xt = {(p.cell, n): x for p in pools for n, x in p.x_all(p.true).items()}
        for k in res["p_true"]:
            out(f"    {k[0]} {k[1]:<14} x={(xt[k] if xt[k] is not None else float('nan')) * 100:+7.2f} %  S={res['S_true'][k]:6.1f} / {res['S_null_mean'][k]:6.2f}  p={res['p_true'][k]:.4f}")
        out(f"  neutral fills (an absent instrument value on a completed row gets the stratum's mean rank): "
            + (", ".join(f"{k[0]} {k[1]} {v}" for k, v in res["neutral"].items() if v) or "none") + f"  (total {sum(res['neutral'].values())})")
        out("  worst-ranked rows (DNF = harness 43's COLLAPSE; the worst rank on every clause of the cell): "
            + (", ".join(f"{c} {a} s{sd} r{rp} {w}" for c, a, sd, rp, w in res["bad_rows"]) or "none"))
        out(f"  slow rows (reported only, ranked by their own values): goodput < {SLOW_FRAC} x the POOLED (BASE + {ta}) median of the cell: "
            + (", ".join(f"{c} {a} s{sd} r{rp} {mb:.1f}" for c, a, sd, rp, mb in res["slow_rows"]) or "none"))
        if res["incomplete"]:
            out("  INCOMPLETE DESIGN (fewer live rows than the design): " + ", ".join(f"{c} {a} {got}/{want}" for c, a, want, got in res["incomplete"]))
        out(f"  min-p (true) = {res['T_true']:.5f} at {res['argmin'][0]} {res['argmin'][1]}")
        out(f"  p = {res['p']:.4f}  (share of the {res['n_perm']} relabellings plus the true labelling whose min-p <= the true min-p)")
        gate = "PASS" if res["p"] > P_PASS else "FAIL"
        if res["incomplete"] or no_verdict_reason:
            # A design with missing rows (a slot voided by a witness and not replaced, a cell that did not run) or beyond the replacement cap has NO
            # verdict: never PASS, whatever p reads; re-run. (DNF and crash rows fill their slots and are the worst rank: they are not "missing".)
            gate = "NO VERDICT"
            why = no_verdict_reason or "the design is incomplete"
            out(f"{gate_name}[rank-minp] NO VERDICT, re-run ({why}; p={res['p']:.4f} is not a verdict)")
        else:
            out(f"{gate_name}[rank-minp] {gate} (p={res['p']:.4f}; {subject} {'may' if gate == 'PASS' else 'may not'} reach main)")
    else:
        res = statistic(rows, "BASE", ta, cells=cells)
        pools = res["pools"]
        out(f"{ta} against BASE (pooled relabelling)")
        for p in pools:
            out(f"  {p.cell}: pool {len(p.ref_rows)} BASE + {len(p.test_rows)} {ta}; relabelled {ta} rows per seed " + " ".join(f"s{s}:{p.k[s]}" for s in p.seeds))
        out(f"  clause x_c(true), sd_c, z_c(true)  (x worse-signed: positive = {ta} worse)")
        for k in res["z_true"]:
            out(f"    {k[0]} {k[1]:<14} x={res['x_true'][k] * 100:+7.2f} %  sd={res['sd'][k] * 100:6.2f} %  z={res['z_true'][k]:+6.2f}")
        out(f"  T(true) = {res['T_true']:+.3f} at {res['arg_true'][0]} {res['arg_true'][1]}")
        out("  share of relabellings whose max is attained at each clause (top 8): "
            + ", ".join(f"{k[0]} {k[1]} {v:.3f}" for k, v in list(res["argmax_share"].items())[:8]))
        out(f"  p = {res['p']:.4f}  (share of {res['n_perm']} relabellings with T >= T(true))")
        gate = "PASS" if res["p"] > P_PASS else "FAIL"
        out(f"{gate_name} {gate} (p={res['p']:.4f}; {subject} {'may' if gate == 'PASS' else 'may not'} reach main)")
    out("")
    out(f"§21 READINGS of {ta} against all BASE rows (reported beside the gate)")
    import place38_parse as pp
    for cell, store in cells:
        xs_, rs_ = live(rows, cell, ta), live(rows, cell, "BASE")
        if xs_ and rs_:
            pp.clause_readings(xs_, rs_, cell, out, f"41b-{cell}@{store}", ta, "BASE")
    wres = None
    if with_wire and any(cnt[(c, "WIRE")] for c, _ in cells):
        out("")
        out("WIRE (RWM_PLACE_WIRE=1), reported only: its own pooled relabelling against BASE")
        wres = statistic(rows, "BASE", "WIRE", cells=cells)
        for k in wres["z_true"]:
            out(f"    {k[0]} {k[1]:<14} x={wres['x_true'][k] * 100:+7.2f} %  sd={wres['sd'][k] * 100:6.2f} %  z={wres['z_true'][k]:+6.2f}")
        out(f"  T(true) = {wres['T_true']:+.3f} at {wres['arg_true'][0]} {wres['arg_true'][1]};  p = {wres['p']:.4f}  (reported, not a gate)")
        for cell, store in cells:
            xs_, rs_ = live(rows, cell, "WIRE"), live(rows, cell, "BASE")
            if xs_ and rs_:
                pp.clause_readings(xs_, rs_, cell, out, f"41b-WIRE-{cell}@{store}", "WIRE", "BASE")
    out("")
    out("TAIL (reported only): lowest goodput per arm and cell against the cell median (all live rows pooled), and the count below 0.5 x median")
    tail_lines(rows, out, cells, arms_all)
    out("")
    out("RUN ORDER (ledger order; every row, any status): idx cell arm seed rep status mbps cpuC/GB cpuS/GB lagC lagS")
    for i, r in enumerate(rows):
        if r["cell"] in dict(cells):
            b = (r.get("bytes") or sp.CELL_SPEC[r["cell"]][3]) / 1e9
            f = lambda k, s=1.0: ("%.1f" % (r[k] / s)) if r.get(k) is not None else "-"  # noqa: E731
            out(f"  {i:>3} {r['cell']} {r['arm']:<4} s{r['seed']} r{r['rep']} {r['status']} {f('mbps')} {f('cpu_cli', b)} {f('cpu_srv', b)} {f('cli_lag_E')} {f('srv_lag_E')}")
    return {"gate": gate, "p": res["p"], "T_true": res["T_true"], "wire_p": wres["p"] if wres else None,
            "argmax_share": res.get("argmax_share"), "z_true": res.get("z_true")}


# The registered design of the merge-candidate guard (status 47): CAND 12 / BASE 12 at every cell, one interleaved session, 6 + 6 per seed.
GUARD_DESIGN = {cell: (12, 12) for cell, _ in CELLS_GUARD}


def report_guard(rows, out, design, no_verdict_reason=None):
    """The merge-candidate guard: rank-minp over the four guard cells, CAND against BASE, WITH A MANDATORY DESIGN. A missing, empty or partial
    design is an error (never a silent PASS); a design with missing live rows reads `NO VERDICT, re-run`."""
    if not design or set(design) != {c for c, _ in CELLS_GUARD}:
        raise ValueError(f"the guard's design= is mandatory and must name every guard cell {[c for c, _ in CELLS_GUARD]}, got {design!r}")
    return report(rows, out, cells=CELLS_GUARD, test_arm="CAND", with_wire=False, method="rank-minp", design=design,
                  title="§41c GUARD ON THE MERGE CANDIDATE", gate_name="GATE-41c", subject="the candidate build", no_verdict_reason=no_verdict_reason)


# ── the guard's resolution, recomputed from THIS session's BASE rows (reported only; status 47) ─────────────────────────────────
# Per cell and clause: the smallest shift of that clause (CAND rows worse by delta: goodput x (1 - delta), the rest x (1 + delta)) that the
# rank-minp gate catches (p <= 0.05) in >= RES_POWER of RES_DRAWS datasets resampled from this session's own BASE rows (BASE and "CAND" drawn
# with replacement from the cell's BASE rows, balanced per seed, at the design's sizes), plus the power at +2.4 % CPU and -5 % goodput.
# N_PERM is small (RES_NPERM) so the whole table costs minutes, not hours; it is a resolution estimate, never part of the verdict.
RES_GRID = [0.01, 0.02, 0.024, 0.03, 0.05, 0.08, 0.12, 0.20, 0.50, 1.00]
RES_DRAWS = 30
RES_NPERM = 1000                 # must be >= 1000: with 26 clauses the gate p cannot reach 0.05 at a few hundred relabellings (floor about 26 / (N + 1))
RES_POWER = 0.8
RES_SEED_DEFAULT = 0             # resolution41c's seed when none is given (printed in its header)
NAMED_SHIFTS = {"cpu_cli/GB": 0.024, "cpu_srv/GB": 0.024, "goodput": 0.05}
_RES = {}


def _res_init(data):
    _RES["data"] = data


def _res_dataset(rng, data, cells, design):
    rows = []
    for cell, _ in cells:
        nb, nc = design[cell]
        for sd in sorted(data[cell]):
            src = data[cell][sd]
            for arm, n in (("BASE", nb // 2), ("CAND", nc // 2)):
                for i in range(n):
                    r = dict(rng.choice(src))
                    r["arm"], r["rep"], r["status"], r["dnf"] = arm, f"{arm}{i}", "LIVE", False
                    rows.append(r)
    return rows


def _res_eval(task):
    cell, clause, delta, draw, cells, design = task
    rng = random.Random(1009 * draw + int(delta * 10000) + sum(map(ord, cell + clause)))
    rows = _res_dataset(rng, _RES["data"], cells, design)
    key = {n: k for n, k, _ in clause_defs(cell)}[clause]
    for r in rows:
        if r["cell"] == cell and r["arm"] == "CAND" and r.get(key) is not None:
            r[key] = r[key] * (1 - delta) if clause == "goodput" else r[key] * (1 + delta)
    return statistic_rank(rows, "BASE", "CAND", n_perm=RES_NPERM, cells=cells)["p"] <= P_PASS


def session_resolution(rows, cells=None, design=None, draws=None, grid=None, procs=None, seed=0):
    """-> {(cell, clause): {"d80": smallest grid delta with power >= RES_POWER or None, "power": {delta: power}}} from this ledger's BASE rows,
    or None if a (cell, seed) has fewer than 2 BASE rows."""
    cells = cells or CELLS_GUARD
    design = design or GUARD_DESIGN
    draws, grid = draws or RES_DRAWS, grid or RES_GRID
    if RES_NPERM < 1000:
        raise ValueError(f"RES_NPERM={RES_NPERM} is too small for a 26-clause gate (needs >= 1000)")
    data = {}
    for cell, _ in cells:
        data[cell] = {}
        for sd in sorted({r["seed"] for r in live(rows, cell, "BASE")}):
            data[cell][sd] = [r for r in live(rows, cell, "BASE") if r["seed"] == sd and not r.get("dnf")]
        if len(data[cell]) < 2 or any(len(v) < 2 for v in data[cell].values()):
            return None
    tasks = [(cell, n, d, k + 100000 * seed, tuple(cells), dict(design)) for cell, _ in cells for n, _, _ in clause_defs(cell) for d in grid for k in range(draws)]
    procs = min(procs or 2, 2, os.cpu_count() or 1)          # at most 2 workers: the step must not push the host into the memory reaper
    res = None
    if procs > 1:
        try:
            import multiprocessing as mp
            with mp.Pool(procs, initializer=_res_init, initargs=(data,)) as pool:
                res = pool.map(_res_eval, tasks, chunksize=8)
        except Exception:                                # no usable process pool here: serial
            res = None
    if res is None:
        _res_init(data)
        res = [_res_eval(t) for t in tasks]
    caught = {}
    for t, hit in zip(tasks, res):
        caught.setdefault((t[0], t[1], t[2]), []).append(hit)
    out = {}
    for cell, _ in cells:
        for n, _, _ in clause_defs(cell):
            pw = {d: sum(caught[(cell, n, d)]) / len(caught[(cell, n, d)]) for d in grid}
            ok = [d for d in grid if pw[d] >= RES_POWER]
            out[(cell, n)] = {"d80": min(ok) if ok else None, "power": pw}
    return out


def resolution_report(rows, out, cells=None, design=None, **kw):
    """Prints the session resolution table (reported only). Deterministic: the same ledger (and seed) gives the same table."""
    cells = cells or CELLS_GUARD
    design = design or GUARD_DESIGN
    des = {c: design[c] for c, _ in cells}
    out(f"RESOLUTION FROM THIS SESSION'S BASE ROWS (reported only; a lower bound on the 10 000-permutation verdict's power (N_PERM {RES_NPERM}); not part of the verdict): BASE rows resampled with replacement to the design "
        f"{des}, balanced per seed; {kw.get('draws') or RES_DRAWS} draws per point, N_PERM {RES_NPERM}, grid "
        f"{[f'{100 * d:g} %' for d in (kw.get('grid') or RES_GRID)]}")
    sr = session_resolution(rows, cells, design, **kw)
    if sr is None:
        out("  not computed: a (cell, seed) has fewer than 2 live BASE rows")
        return None
    out(f"  per clause: smallest shift caught in >= {int(100 * RES_POWER)} % of draws (d80), and the power at the named shifts (client / server CPU +2.4 %, goodput -5 %)")
    for (cell, n), v in sr.items():
        nm = f"  power at {100 * NAMED_SHIFTS[n]:g} % = {v['power'].get(NAMED_SHIFTS[n], float('nan')):.2f}" if n in NAMED_SHIFTS else ""
        out(f"  {cell:<8} {n:<14} d80 = {('%g %%' % (100 * v['d80'])) if v['d80'] is not None else '> %g %%' % (100 * max(kw.get('grid') or RES_GRID))}{nm}")
    return sr


def main(argv):
    if len(argv) < 2 or argv[0] not in ("score", "dry"):
        print(__doc__)
        return 2
    import place38_parse as pp
    args, method, cells, ta = list(argv[1:]), "relabel-max", None, "MAIN"
    if "--method" in args:
        i = args.index("--method")
        method = args[i + 1]
        del args[i:i + 2]
    if "--guard" in args:           # the merge-candidate guard: four cells, CAND against BASE, no WIRE
        args.remove("--guard")
        cells, ta = CELLS_GUARD, "CAND"
    rows, _ = pp.rows_of(args)
    report(rows, dry=argv[0] == "dry", cells=cells, test_arm=ta, with_wire=(ta == "MAIN"), method=method)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
