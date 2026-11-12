#!/usr/bin/env python3
"""Plan 40 (docs/status.md §40): the p90 dispersion estimator, fixed in advance, with aggregation (netem `slot`)
and policer (`tbf`) cells. Built on disp36.py: the same unfiltered `-Q in` capture, drain/settle, windowed [SNAP]
witnesses, GSO-aware b, kept per-skb tables and ABORT-BIN battery. Only the estimator and the readouts differ.

Estimator (the only one, fixed now; it uses no L): per leg, all skbs in arrival order, b as §36, after a 1 s
warm-up. For each 250 ms window, the p90 of r_4(j) = sum_{i=j+1..j+4} b_i / (t_{j+4} - t_j) over the trains starting
in it. The row value L-hat is the median over windows. D = delivered bytes / time from the same capture.

Readouts (per cell and leg; Q(p, n) = mean over the cap's live rows of L-hat / sustained rate):
  E1  Q in [0.90, 1.10] at every cap >= the crossover      E2  Q <= 1.10 at every cap
  E3  (1 + q) L-hat / D >= 1.2 for every row at every cap below the crossover (q = 0.1; the cap's minimum reported)
  E4  at every cap >= the crossover the 4 rows' L-hat agree within max/min <= 1.05
  E5  (reported) within-row cv over windows, in-run share, the same readouts for the window median
  E6  (reported) medians of the p<i>: fields sig_us, rvar_us, qsp_us, msd_us, tlag_us per cap
Crossovers: 128 on c1s, c2 and c8 fast; 256 on c8 slow. c2slot / c2tbf: E1 and E2 against their sustained rates,
reported; AGG-SAFE iff both hold E2, else AGG-OVER-READ with the factor.
Verdict: P90-READS-LINK iff E1-E4 hold at c1s, c2 and both c8 legs (+ the aggregation line), else NOT-YET.

Interpretations the spec leaves open: a cap with too few live rows for a clause (no row for E1/E2/E3, fewer than 4
for E4) is UNSCOREABLE, which fails the clause; D = bytes of post-warm-up skbs / (last stamp - warm-up end);
the in-run share of c2tbf is read against the netem rate (the pipe in front of the policer).
"""
import json
import os
import re
import sys
from collections import Counter

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import l1common as lc  # noqa: E402
import disp36 as d36  # noqa: E402

K = 4
Q_GAIN = 1.1                          # (1 + q), q = 0.1 at Bulk
E1_LO, E1_HI = 0.90, 1.10
E2_MAX = 1.10
E3_MIN = 1.2
E4_MAX = 1.05

CELLS40 = ["c1s-400", "c2-100", "c8-100", "c2slot-100", "c2tbf-100"]
SCORED40 = ["c1s-400", "c2-100", "c8-100"]
AGG40 = ["c2slot-100", "c2tbf-100"]
GRID40 = {
    "c1s-400": [16, 32, 64, 128, 256, 512, 1024, 2048],
    "c2-100": [16, 32, 64, 128, 256, 512],
    "c8-100": [16, 32, 64, 128, 256, 512, 1024],
    "c2slot-100": [32, 128, 512],
    "c2tbf-100": [32, 128, 512],
}
RATE40 = {                            # sustained rate per leg, bytes/s
    "c1s-400": [125e6], "c2-100": [12.5e6], "c8-100": [12.5e6, 2.5e6],
    "c2slot-100": [12.5e6], "c2tbf-100": [6.25e6],
}
CROSS40 = {                           # crossover cap per leg (fixed from §36)
    "c1s-400": [128], "c2-100": [128], "c8-100": [128, 256], "c2slot-100": [128], "c2tbf-100": [128],
}
BLOCKS40 = [("42", 1), ("7", 1), ("42", 2), ("7", 2)]
SPEC40 = {                            # cell -> (scenA, scenB, mode, bytes), merged into stage3_parse.CELL_SPEC
    "c2slot-100": ("c2slot", "c2slot", "single", 100000000),
    "c2tbf-100": ("c2tbf", "c2tbf", "single", 100000000),
}
for _c, _r in (("c2slot-100", [12.5e6]), ("c2tbf-100", [12.5e6])):      # in-run share against the netem rate
    d36.LINKS36[_c] = _r
FIELDS40 = ("sig_us", "rvar_us", "qsp_us", "msd_us", "tlag_us")
_E6 = re.compile(r"\bp(\d+):infl=.*?sig_us=(\S+?)/n\d+ rvar_us=(\S+?)/n\d+ qsp_us=(\S+?)/n\d+ msd_us=(\S+?)/n\d+ tlag_us=(\S+?)/n\d+")


def plan40(smoke=False):
    """[(cell, arm, seed, rep)] in run order: 4 blocks (42,1) (7,1) (42,2) (7,2); ascending on even blocks and
    descending on odd; the cell order alternates (reversed on odd blocks). 27 caps x 4 rows = 108."""
    if smoke:
        return [("c2-100", "CAP128", "42", 1), ("c2tbf-100", "CAP128", "42", 1)]
    out = []
    for b, (seed, rep) in enumerate(BLOCKS40):
        for cell in (CELLS40 if b % 2 == 0 else CELLS40[::-1]):
            g = GRID40[cell] if b % 2 == 0 else GRID40[cell][::-1]
            out += [(cell, f"CAP{n}", seed, rep) for n in g]
    return out


EXPECT_ROWS40 = len(plan40())


# ── the estimator ─────────────────────────────────────────────────────────
def estimate40(skbs):
    """{lhat, wmed, cv, nwin, zero_dt, D, span_s}: L-hat = median over complete 250 ms windows of the window p90 of
    r_4 (bytes/s); wmed the same with the window median; cv over the windows' p90s; D = delivered bytes/s."""
    res = {"lhat": None, "wmed": None, "cv": None, "nwin": 0, "zero_dt": 0, "D": None, "span_s": None}
    n = len(skbs)
    if n < K + 1:
        return res
    ts = [t for t, _ in skbs]
    pre = [0]
    for _, b in skbs:
        pre.append(pre[-1] + b)
    t0, tlast = ts[0] + d36.WARM_NS, ts[-1]
    wins, zero = {}, 0
    for j in range(n - K):
        if ts[j] < t0:
            continue
        dt = ts[j + K] - ts[j]
        if dt <= 0:
            zero += 1
            continue
        wins.setdefault((ts[j] - t0) // d36.WIN_NS, []).append((pre[j + K + 1] - pre[j + 1]) * d36.NS / dt)
    keep = [w for w in sorted(wins) if t0 + (w + 1) * d36.WIN_NS <= tlast]
    res["zero_dt"], res["nwin"] = zero, len(keep)
    if keep:
        p9 = [d36._pct(wins[w], 0.9) for w in keep]
        res["lhat"] = d36._median(p9)
        res["wmed"] = d36._median([d36._median(wins[w]) for w in keep])
        mean = sum(p9) / len(p9)
        sd = (sum((x - mean) ** 2 for x in p9) / len(p9)) ** 0.5
        res["cv"] = sd / mean if mean > 0 else None
    if tlast > t0:
        post = sum(b for t, b in skbs if t >= t0)
        res["D"] = post * d36.NS / (tlast - t0)
        res["span_s"] = (tlast - t0) / d36.NS
    return res


def e6_medians(cli, nleg, warm_s=1.0):
    """Per leg, post-warm-up medians of the p<i>: fields sig_us, rvar_us, qsp_us, msd_us, tlag_us ([DIAG], µs)."""
    ser = {i: {f: [] for f in FIELDS40} for i in range(nleg)}
    for ln in cli:
        m = d36._DIAG_T.match(ln)
        if not m or float(m.group(1)) < warm_s:
            continue
        for g in _E6.findall(ln):
            i = int(g[0])
            if i not in ser:
                continue
            for f, v in zip(FIELDS40, g[1:]):
                try:
                    ser[i][f].append(float(v))
                except ValueError:
                    pass
    return [{f: d36._median(ser[i][f]) for f in FIELDS40} for i in range(nleg)]


def row_fields(cell, arm, drv, cli, pcap_dir=None, table_dir=None, row_name=None, srv=None):
    """-> (d40 dict, problems, notes). The §36 capture/witness machinery (disp36.row_fields) plus the p90
    estimator, D and the E6 fields per live leg."""
    d, problems, notes = d36.row_fields(cell, arm, drv, cli, pcap_dir, table_dir, row_name, srv)
    nleg = len(RATE40[cell])
    e6 = e6_medians(cli, nleg)
    for i in range(nleg):
        leg = d["legs"][i]
        leg["e6"] = e6[i]
        if leg.get("state") != "live":
            continue
        recs, _ = d36.read_pcap(os.path.join(pcap_dir or "", f"p{i}.pcap"))
        leg["p90"] = estimate40(d36.skb_series(recs, d["m"]))
    return d, problems, notes


# ── scoring a ledger ──────────────────────────────────────────────────────
def _row_leg_est(r, leg):
    lg = d36.live_leg(r, leg)
    return (lg or {}).get("p90")


def cap_rows40(rows, cell, n):
    return [r for r in rows if r["cell"] == cell and r["arm"] == f"CAP{n}" and r["status"] == "LIVE"]


def per_cap(rows, cell, leg):
    """{n: {rows, Q, Qmin, Qmax, Qwmed, e3 [per-row (1+q)Lhat/D], lhats, cv, share, e6}}"""
    rate = RATE40[cell][leg]
    out = {}
    for n in GRID40[cell]:
        ests, shares, e6 = [], [], {f: [] for f in FIELDS40}
        for r in cap_rows40(rows, cell, n):
            e = _row_leg_est(r, leg)
            if not e or e.get("lhat") is None:
                continue
            ests.append(e)
            lg = d36.live_leg(r, leg)
            shares.append((lg.get("d3") or {}).get("share"))
            for f in FIELDS40:
                v = ((lg.get("e6") or {}).get(f))
                if v is not None:
                    e6[f].append(v)
        if not ests:
            continue
        qs = [e["lhat"] / rate for e in ests]
        out[n] = {
            "rows": len(ests), "Q": sum(qs) / len(qs), "Qmin": min(qs), "Qmax": max(qs),
            "Qwmed": d36._mean([e["wmed"] / rate for e in ests if e.get("wmed") is not None]),
            "e3": [Q_GAIN * e["lhat"] / e["D"] for e in ests if e.get("D")],
            "lhats": [e["lhat"] for e in ests], "cv": d36._mean([e["cv"] for e in ests]),
            "share": d36._mean(shares), "e6": {f: d36._median(v) for f, v in e6.items()},
        }
    return out


def clauses(pc, cell, leg):
    """-> {E1, E2, E3, E4: PASS|FAIL|UNSCOREABLE} for one cell/leg."""
    cross = CROSS40[cell][leg]
    grid = GRID40[cell]
    above = [n for n in grid if n >= cross]
    below = [n for n in grid if n < cross]

    def ok(cond, caps, need):
        if any(n not in pc or not need(pc[n]) for n in caps):
            return "UNSCOREABLE"
        return "PASS" if all(cond(pc[n]) for n in caps) else "FAIL"
    res = {
        "E1": ok(lambda c: E1_LO <= c["Q"] <= E1_HI, above, lambda c: True),
        "E2": ok(lambda c: c["Q"] <= E2_MAX, grid, lambda c: True),
        "E3": ok(lambda c: min(c["e3"]) >= E3_MIN, below, lambda c: bool(c["e3"])) if below else "PASS",
        "E4": ok(lambda c: max(c["lhats"]) / min(c["lhats"]) <= E4_MAX, above, lambda c: c["rows"] >= 4),
    }
    return res


def fmt(x, nd=3):
    return d36.fmt(x, nd)


def score_rows(rows, out=print):
    out("P90 DISPERSION DIAGNOSTIC PLAN 40 SCORE (estimator fixed in advance; nothing here ships)")
    st = Counter((r["cell"], r["status"]) for r in rows)
    out(f"ROWS {len(rows)}/{EXPECT_ROWS40} status counts {sorted(st.items())}")
    for r in rows:
        if r["status"] != "LIVE" or r.get("notes") or r.get("io_high"):
            out(f"  {r['cell']} {r['arm']} s{r['seed']} r{r['rep']}: {r['status']} problems={r.get('problems')} "
                f"notes={r.get('notes')} io_full10={r.get('io_full10')}")
    res = {"cells": {}}
    for cell in CELLS40:
        role = "scored" if cell in SCORED40 else "aggregation/policer, reported"
        out(f"CELL {cell} ({role}); sustained {RATE40[cell]} B/s; crossover {CROSS40[cell]}; caps {GRID40[cell]}")
        cres = {"legs": {}}
        for leg in range(len(RATE40[cell])):
            pc = per_cap(rows, cell, leg)
            cl = clauses(pc, cell, leg)
            out(f"  LEG p{leg}  (Q = mean L-hat/sustained; E3 = min over rows of (1+q) L-hat/D; E4 = max/min of the rows' L-hat)")
            out("    n      rows  Q       Qmin    Qmax    Q(wmed)  E3min   E4     cv      in-run  sig_us  rvar_us qsp_us  msd_us  tlag_us")
            lres = {"clauses": cl, "Q": {}, "E3min": {}, "E4": {}}
            for n in GRID40[cell]:
                c = pc.get(n)
                if not c:
                    out(f"    {n:<6} 0     -")
                    continue
                e3 = min(c["e3"]) if c["e3"] else None
                e4 = max(c["lhats"]) / min(c["lhats"]) if c["rows"] >= 2 else None
                lres["Q"][str(n)], lres["E3min"][str(n)], lres["E4"][str(n)] = c["Q"], e3, e4
                out(f"    {n:<6} {c['rows']:<5} {fmt(c['Q'])} {fmt(c['Qmin']):>7} {fmt(c['Qmax']):>7} {fmt(c['Qwmed']):>8} "
                    f"{fmt(e3, 2):>7} {fmt(e4, 3):>6} {fmt(c['cv']):>7} {fmt(c['share']):>7}  "
                    + "  ".join(f"{fmt(c['e6'][f], 0):>6}" for f in FIELDS40))
            out(f"    E1 {cl['E1']}  E2 {cl['E2']}  E3 {cl['E3']}  E4 {cl['E4']}")
            lres["E6"] = {str(n): pc[n]["e6"] for n in pc}
            cres["legs"][str(leg)] = lres
        res["cells"][cell] = cres
    # aggregation and policer
    qmax = {}
    for cell in AGG40:
        pc = per_cap(rows, cell, 0)
        qmax[cell] = max((c["Q"] for c in pc.values()), default=None)
        cl = res["cells"][cell]["legs"]["0"]["clauses"]
        out(f"AGG {cell}: E1 {cl['E1']} E2 {cl['E2']}; max Q over caps {fmt(qmax[cell], 2)} x sustained "
            f"(per cap " + ", ".join(f"{n}:{fmt(c['Q'], 2)}" for n, c in sorted(pc.items())) + ")")
    e2 = [res["cells"][c]["legs"]["0"]["clauses"]["E2"] for c in AGG40]
    if all(x == "PASS" for x in e2):
        agg = "AGG-SAFE"
    elif any(x == "UNSCOREABLE" for x in e2) and not any(x == "FAIL" for x in e2):
        agg = "AGG-UNSCOREABLE"
    else:
        agg = "AGG-OVER-READ"
    factor = max([v for v in qmax.values() if v is not None], default=None)
    out(f"{agg}" + (f" (factor up to x{fmt(factor, 2)})" if agg == "AGG-OVER-READ" else ""))
    # the verdict
    failed = []
    for cell in SCORED40:
        for leg in range(len(RATE40[cell])):
            cl = res["cells"][cell]["legs"][str(leg)]["clauses"]
            for e in ("E1", "E2", "E3", "E4"):
                if cl[e] != "PASS":
                    failed.append({"cell": cell, "leg": leg, "clause": e, "got": cl[e]})
    verdict = "NOT-YET" if failed else "P90-READS-LINK"
    out(f"VERDICT {verdict} (E1-E4 at c1s, c2 and both legs of c8)")
    for f in failed:
        out(f"  failed {f['cell']} p{f['leg']} {f['clause']}={f['got']}")
    if verdict == "P90-READS-LINK" and agg == "AGG-OVER-READ":
        out("  with AGG-OVER-READ: the law also needs an aggregation bound (a windowed excess, as BBR's extra_acked) before any build")
    res.update({"verdict": verdict, "agg": agg, "agg_factor": factor, "failed": failed})
    out("RESULT " + json.dumps(res, sort_keys=True, default=str))
    return res


def score_paths(paths, out=print):
    import loopdiag_parse as ld
    return score_rows(ld.rows_of(paths), out=out)


def main(argv):
    if len(argv) >= 2 and argv[1] == "plan40":
        for cell, arm, seed, rep in plan40(smoke=len(argv) > 2 and argv[2] == "smoke"):
            print(f"{cell} {arm} {seed} {rep}")
        return 0
    print(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
