#!/usr/bin/env python3
"""The §41b guard: an A/A-calibrated, one-sided, pooled-relabelling statistic (docs/status.md "## 41b").

    place41b_score.py score <ledger>...     the real scoring: arms BASE (12 per cell), MAIN (6), WIRE (6, reported only)
    place41b_score.py dry <ledger>...       the same code on a ledger with fewer BASE rows (§41's: BASE 6, MAIN 6 per cell)

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
CELLS = [("c1s-400", "def"), ("c1d-400", "def")]


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
        b = sp.CELL_SPEC[cell][3]
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


class Pool:
    """One cell's pooled rows (reference arm first, tested arm after) and the true selection."""

    def __init__(self, rows, cell, ref_arm, test_arm):
        self.cell = cell
        self.ref_rows, self.test_rows = live(rows, cell, ref_arm), live(rows, cell, test_arm)
        self.rows = self.ref_rows + self.test_rows
        self.true = list(range(len(self.ref_rows), len(self.rows)))
        self.defs = clause_defs(cell)
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


def statistic(rows, ref_arm, test_arm, n_perm=N_PERM, seed=RNG_SEED):
    pools = [Pool(rows, cell, ref_arm, test_arm) for cell, _ in CELLS]
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


def resolution(rows, arm="BASE"):
    """BASE alone: per cell and clause the q95 of |x_c| over the half-per-seed splits, beside the §21 tolerance."""
    out = []
    for cell, _ in CELLS:
        base = live(rows, cell, arm)
        defs = clause_defs(cell)
        vals = {n: [value(r, cell, k) for r in base] for n, k, _ in defs}
        seeds = sorted({r["seed"] for r in base})
        groups = {s: [i for i, r in enumerate(base) if r["seed"] == s] for s in seeds}
        splits = [sum(c, ()) for c in itertools.product(*[list(itertools.combinations(groups[s], len(groups[s]) // 2)) for s in seeds])]
        rel = {"c1s-400": {"gp": 0.049, "cpu": 0.024}, "c1d-400": {"gp": 0.056, "cpu": 0.065}}[cell]
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


def report(rows, out=print, dry=False):
    out(f"§41b GUARD ({'DRY RUN on a ledger with fewer BASE rows' if dry else 'real scoring'}): N_PERM={N_PERM} RNG seed {RNG_SEED}, one-sided, "
        f"max over every clause of both cells, PASS iff p > {P_PASS}")
    cnt = {(c, a): len(live(rows, c, a)) for c, _ in CELLS for a in ("BASE", "MAIN", "WIRE")}
    out("LIVE ROWS " + "  ".join(f"{c} {a}={cnt[(c, a)]}" for c, _ in CELLS for a in ("BASE", "MAIN", "WIRE")))
    out("")
    out("RESOLUTION (computed from BASE rows alone, before MAIN is read): per clause the q95 of |x_c| over the half-per-seed splits of BASE, beside the §21 tolerance")
    for r in resolution(rows):
        q = r["q95"]
        out(f"  {r['cell']} {r['clause']:<14} q95|x|={('%.4f' % q) if q is not None else '-'} (splits={r['splits']}, nBASE={r['n_base']})  §21 tol={r['tol']:.4f}  "
            + ("" if q is None else ("q95 > tol: the guard cannot see this clause below q95" if q > r["tol"] else "q95 <= tol")))
    out("")
    res = statistic(rows, "BASE", "MAIN")
    pools = res["pools"]
    out("MAIN against BASE (pooled relabelling)")
    for p in pools:
        out(f"  {p.cell}: pool {len(p.ref_rows)} BASE + {len(p.test_rows)} MAIN; relabelled MAIN rows per seed " + " ".join(f"s{s}:{p.k[s]}" for s in p.seeds))
    out("  clause x_c(true), sd_c, z_c(true)  (x worse-signed: positive = MAIN worse)")
    for k in res["z_true"]:
        out(f"    {k[0]} {k[1]:<14} x={res['x_true'][k] * 100:+7.2f} %  sd={res['sd'][k] * 100:6.2f} %  z={res['z_true'][k]:+6.2f}")
    out(f"  T(true) = {res['T_true']:+.3f} at {res['arg_true'][0]} {res['arg_true'][1]}")
    out("  share of relabellings whose max is attained at each clause (top 8): "
        + ", ".join(f"{k[0]} {k[1]} {v:.3f}" for k, v in list(res["argmax_share"].items())[:8]))
    out(f"  p = {res['p']:.4f}  (share of {res['n_perm']} relabellings with T >= T(true))")
    gate = "PASS" if res["p"] > P_PASS else "FAIL"
    out(f"GATE-41b {gate} (p={res['p']:.4f}; the gate-off binary {'may' if gate == 'PASS' else 'may not'} reach main)")
    out("")
    out("§21 READINGS of MAIN against all BASE rows (reported beside the gate)")
    import place38_parse as pp
    for cell, store in CELLS:
        xs_, rs_ = live(rows, cell, "MAIN"), live(rows, cell, "BASE")
        if xs_ and rs_:
            pp.clause_readings(xs_, rs_, cell, out, f"41b-{cell}@{store}", "MAIN", "BASE")
    wres = None
    if any(cnt[(c, "WIRE")] for c, _ in CELLS):
        out("")
        out("WIRE (RWM_PLACE_WIRE=1), reported only: its own pooled relabelling against BASE")
        wres = statistic(rows, "BASE", "WIRE")
        for k in wres["z_true"]:
            out(f"    {k[0]} {k[1]:<14} x={wres['x_true'][k] * 100:+7.2f} %  sd={wres['sd'][k] * 100:6.2f} %  z={wres['z_true'][k]:+6.2f}")
        out(f"  T(true) = {wres['T_true']:+.3f} at {wres['arg_true'][0]} {wres['arg_true'][1]};  p = {wres['p']:.4f}  (reported, not a gate)")
        for cell, store in CELLS:
            xs_, rs_ = live(rows, cell, "WIRE"), live(rows, cell, "BASE")
            if xs_ and rs_:
                pp.clause_readings(xs_, rs_, cell, out, f"41b-WIRE-{cell}@{store}", "WIRE", "BASE")
    out("")
    out("RUN ORDER (ledger order; every row, any status): idx cell arm seed rep status mbps cpuC/GB cpuS/GB lagC lagS")
    for i, r in enumerate(rows):
        if r["cell"] in dict(CELLS):
            b = sp.CELL_SPEC[r["cell"]][3] / 1e9
            f = lambda k, s=1.0: ("%.1f" % (r[k] / s)) if r.get(k) is not None else "-"  # noqa: E731
            out(f"  {i:>3} {r['cell']} {r['arm']:<4} s{r['seed']} r{r['rep']} {r['status']} {f('mbps')} {f('cpu_cli', b)} {f('cpu_srv', b)} {f('cli_lag_E')} {f('srv_lag_E')}")
    return {"gate": gate, "p": res["p"], "T_true": res["T_true"], "wire_p": wres["p"] if wres else None,
            "argmax_share": res["argmax_share"], "z_true": res["z_true"]}


def main(argv):
    if len(argv) < 2 or argv[0] not in ("score", "dry"):
        print(__doc__)
        return 2
    import place38_parse as pp
    rows, _ = pp.rows_of(argv[1:])
    report(rows, dry=argv[0] == "dry")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
