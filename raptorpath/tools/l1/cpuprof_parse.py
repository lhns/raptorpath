#!/usr/bin/env python3
"""Collate the CPU-ceiling battery's ledger into the two tables the contract
is scored on.

    python3 cpuprof_parse.py [--calib] <ledger.log> [<ledger.log> ...]

Reads ONLY lines this battery's driver wrote (`CEIL`, `CPUPROFLINE`,
`PERFCAP`, `LIVENESS`, `ABORT`, `INSTRUMENT-FAIL-*`, `ARM-CONTAMINATION`), so
it can be run against a committed ledger with no VM, no binary and no perf.

WHAT IT DOES NOT DO, deliberately:

  * It does not RANK the seams into "top two". That is a verdict, it belongs
    to the results section, and it is taken against the pre-registered
    operational definition (a seam is a top-two cost iff its share of process
    CPU is >= the contract's bar AND it is one of the two largest). This
    script prints the shares; a human applies the bar.
  * It does not merge `perf`'s symbol table with the `[CPUPROF]` seams. They
    are different attributions of the same CPU by different mechanisms and
    the contract scores them AGAINST each other; averaging them would destroy
    exactly the cross-check they exist for.
  * It computes no significance. n is small by construction here.
"""
import argparse
import re
import statistics
import sys
from collections import defaultdict

SEAMS = ["enc", "src", "frm", "ser", "hand"]


def _f(v):
    try:
        return float(v)
    except (TypeError, ValueError):
        return None


def parse(paths):
    """-> (rows, cpuprof, perfcap, problems). Keyed by (cell, arm)."""
    rows = defaultdict(list)      # (cell, arm) -> [dict of CEIL fields]
    cpuprof = defaultdict(list)   # (cell, arm) -> [dict of seam readings]
    perfcap = defaultdict(list)
    problems = []
    for p in paths:
        with open(p, "r", errors="replace") as fh:
            for line in fh:
                line = line.rstrip("\n")
                if line.startswith("CEIL "):
                    toks = line.split()
                    name = toks[1]
                    kv = dict(t.split("=", 1) for t in toks[2:] if "=" in t)
                    cell, _, arm = name.rpartition("-")
                    rows[(cell, arm)].append(
                        {k: _f(v) if k != "rep" else v for k, v in kv.items()}
                    )
                elif line.startswith("CPUPROFLINE "):
                    toks = line.split()
                    name = toks[1]
                    cell, _, arm = name.rpartition("-")
                    rec = {}
                    for t in toks:
                        if "=" not in t:
                            continue
                        k, v = t.split("=", 1)
                        if "/" in v and v.count("/") == 2:
                            ms, n, share = v.split("/")
                            rec[k] = {
                                "ms": _f(ms),
                                "n": _f(n.lstrip("n")),
                                "share": _f(share),
                            }
                        else:
                            rec[k] = _f(v)
                    cpuprof[(cell, arm)].append(rec)
                elif line.startswith("PERFCAP "):
                    toks = line.split()
                    cell, _, arm = toks[1].rpartition("-")
                    kv = dict(t.split("=", 1) for t in toks[2:] if "=" in t)
                    perfcap[(cell, arm)].append(kv)
                elif line.startswith(("ABORT ", "INSTRUMENT-FAIL", "ARM-CONTAMINATION",
                                      "ARM-LIVENESS-FAIL", "MISSING-BINARY",
                                      "ARM-VANISHED", "QCAP-MISSING")):
                    problems.append(line)
                elif line.startswith("=== TEXT-EQUAL"):
                    problems.append(line[4:])
    return rows, cpuprof, perfcap, problems


def agg(vals):
    vals = [v for v in vals if v is not None]
    if not vals:
        return None, None, 0
    if len(vals) == 1:
        return vals[0], 0.0, 1
    return statistics.mean(vals), 2 * statistics.stdev(vals), len(vals)


def fmt(m, s, n, prec=2):
    if m is None:
        return "-"
    return f"{m:.{prec}f} (2s {s:.{prec}f}, n={n})"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("logs", nargs="+")
    ap.add_argument("--calib", action="store_true",
                    help="calibration mode: print the smoke checklist and the "
                         "two headroom columns; score nothing")
    ap.add_argument("--nproc", type=int, default=6,
                    help="cores on the box, for the CPU-headroom column")
    a = ap.parse_args()

    rows, cpuprof, perfcap, problems = parse(a.logs)
    if not rows:
        print("NO CEIL ROWS — the battery produced no parseable invocation", file=sys.stderr)
        return 1

    # ── THE PROBLEM TABLE FIRST, ALWAYS. A decomposition read before its
    #    instrument-failure list is a decomposition of an unknown subset.
    print("=" * 78)
    print("INSTRUMENT AND ABORT TABLE — READ THIS BEFORE ANY NUMBER BELOW")
    print("=" * 78)
    if problems:
        for p in problems:
            print("  " + p)
    else:
        print("  (none)")
    print()

    # ── THE CEILING TABLE. One row per (cell, arm).
    print("=" * 78)
    print("THE CEILING — ms/MB, cores, and the predicted-vs-measured goodput")
    print("=" * 78)
    hdr = f"{'cell-arm':<10} {'ms/MB':<24} {'cores':<22} {'pred Mbit/s':<22} {'meas Mbit/s':<22}"
    print(hdr)
    print("-" * len(hdr))
    base = {}
    for (cell, arm) in sorted(rows):
        r = rows[(cell, arm)]
        ms = agg([x.get("ms_per_MB") for x in r])
        co = agg([x.get("cores") for x in r])
        pr = agg([x.get("pred_mbit") for x in r])
        me = agg([x.get("meas_mbit") for x in r])
        if arm == "B":
            base[cell] = (ms[0], me[0], co[0])
        print(f"{cell + '-' + arm:<10} {fmt(*ms):<24} {fmt(*co, 3):<22} "
              f"{fmt(*pr, 1):<22} {fmt(*me, 1):<22}")
    print()
    print("  ms/MB = CPUCLI_s * 1000 / (bytes/1e6);  cores = CPUCLI_s / TRANSFER seconds")
    print("  pred  = cores / ms_per_MB * 8000   <- the c9 ceiling arithmetic, verbatim")
    print("  THE CEILING IS ARM B's. S and P carry their instrument's own cost.")
    print()

    # ── THE INSTRUMENT COST. The number that makes the other two readable.
    if base:
        print("=" * 78)
        print("INSTRUMENT COST — each arm against its own cell's B, paired by cell")
        print("=" * 78)
        print(f"{'cell-arm':<10} {'d ms/MB':<12} {'d ms/MB %':<12} {'d goodput %':<14}")
        print("-" * 50)
        for (cell, arm) in sorted(rows):
            if arm == "B" or cell not in base:
                continue
            b_ms, b_me, _ = base[cell]
            ms = agg([x.get("ms_per_MB") for x in rows[(cell, arm)]])[0]
            me = agg([x.get("meas_mbit") for x in rows[(cell, arm)]])[0]
            d_ms = (ms - b_ms) if (ms is not None and b_ms) else None
            p_ms = (100 * d_ms / b_ms) if (d_ms is not None and b_ms) else None
            p_me = (100 * (me - b_me) / b_me) if (me is not None and b_me) else None
            print(f"{cell + '-' + arm:<10} "
                  f"{('%+.2f' % d_ms) if d_ms is not None else '-':<12} "
                  f"{('%+.1f' % p_ms) if p_ms is not None else '-':<12} "
                  f"{('%+.1f' % p_me) if p_me is not None else '-':<14}")
        print()

    # ── THE DECOMPOSITION. Shares of PROCESS CPU, with unattr first-class.
    print("=" * 78)
    print("THE DECOMPOSITION — [CPUPROF] seam shares of PROCESS CPU (arm S)")
    print("=" * 78)
    if not cpuprof:
        print("  NO [CPUPROF] LINES PARSED — the self-timing arm produced nothing.")
    for (cell, arm) in sorted(cpuprof):
        recs = cpuprof[(cell, arm)]
        print(f"\n  {cell}-{arm}  (n={len(recs)} lines)")
        cores = agg([r.get("cores") for r in recs])
        print(f"    run_ms {fmt(*agg([r.get('run_ms') for r in recs]), 1)}   "
              f"cpu_ms {fmt(*agg([r.get('cpu_ms') for r in recs]), 1)}   "
              f"cores {fmt(*cores, 3)}")
        print(f"    {'seam':<8} {'share of CPU':<22} {'ms':<20} {'entries':<16} {'us/call':<10}")
        print("    " + "-" * 74)
        for s in SEAMS:
            sh = agg([r.get(s, {}).get("share") for r in recs if isinstance(r.get(s), dict)])
            ms = agg([r.get(s, {}).get("ms") for r in recs if isinstance(r.get(s), dict)])
            n = agg([r.get(s, {}).get("n") for r in recs if isinstance(r.get(s), dict)])
            per = (1000.0 * ms[0] / n[0]) if (ms[0] and n[0]) else None
            print(f"    {s:<8} {fmt(*sh, 4):<22} {fmt(*ms, 1):<20} "
                  f"{fmt(*n, 0):<16} {('%.2f' % per) if per else '-':<10}")
        attr = agg([r.get("attr") for r in recs])
        un = agg([r.get("unattr") for r in recs])
        print("    " + "-" * 74)
        print(f"    {'attr':<8} {fmt(*attr, 4)}")
        print(f"    {'unattr':<8} {fmt(*un, 4)}   <- NOT an error term. quinn's driver,")
        print(f"    {'':<8} its sendmsg, and rustls/ring AEAD all live here, and NONE")
        print(f"    {'':<8} of them is reachable from the sender task. perf must explain it.")
    print()

    # ── perf's own accounting.
    if perfcap:
        print("=" * 78)
        print("PERF CAPTURE — attach gap and symbolization")
        print("=" * 78)
        for (cell, arm) in sorted(perfcap):
            for rec in perfcap[(cell, arm)]:
                print(f"  {cell}-{arm} rep={rec.get('rep', '?')} "
                      f"attach_ms={rec.get('attach_ms')} "
                      f"symbol_rows={rec.get('symbol_rows')} "
                      f"graph={rec.get('graph')} data={rec.get('data')}")
        print()
        print("  The attach gap is the head of the run perf MISSED. P is read as a")
        print("  SHAPE (which symbols dominate), never as a total; the total is B's.")
        print("  Leaf profiles are beside the .data files as *.report.txt.")
        print()

    if a.calib:
        print("=" * 78)
        print("CALIBRATION — n = 1. NOTHING ABOVE OR BELOW IS A RESULT.")
        print("=" * 78)
        for (cell, arm) in sorted(rows):
            r = rows[(cell, arm)][0]
            co = r.get("cores")
            cpu_head = (100 * (1 - co / a.nproc)) if co else None
            print(f"  {cell}-{arm}: cores={co} of {a.nproc} -> "
                  f"CPU headroom {('%.1f %%' % cpu_head) if cpu_head is not None else '-'}")
        print()
        print("  LINK headroom comes from the -q.txt qdisc captures (discipline 16,")
        print("  TRANSFER wall denominator) and is filled in by hand into the")
        print("  contract's table, exactly as every other calibration in this tree.")
        print("  AT A SENDER-BOUND CELL THE BINDING PERMISSION IS THE CPU COLUMN,")
        print("  and c9 is the cell where the two disagree by construction.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
