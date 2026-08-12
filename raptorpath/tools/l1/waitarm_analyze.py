#!/usr/bin/env python3
"""THE SENDER'S OWN WAIT ATTRIBUTION, per rep, read from logs already in the tree.

goal-gate "The Latency-Feedback Source". NO VM RUN: every number this prints is
a field of a per-rep summary record already committed under
`raptorpath/docs/l1-raw/`. The records carry the eight `wait[...]` buckets
(`net/mod.rs:5824-5829`, parsed by `hi_parse.py:160-167` as the MEDIAN over the
rep's DIAG windows) and no section of the ledger had read them per rep.

The question it answers is the one "The Queue Fix"'s RANK 1 handover asked and
did not measure: WHAT IS THE OFFERED LOAD DOING at the duals, and what does the
sender loop look like in the c8 reps that collapse?

Usage:  python3 waitarm_analyze.py [l1-raw-dir]
"""
import glob
import json
import os
import re
import statistics as st
import sys

RE = re.compile(r"(?:FLIPRESULT|HIRESULT|UNIRESULT|RESULT)\s+(\{.*\})")

# The `[SF]`-era arms -- the ones every store-cap section in the ledger is
# scored on. B/D/H/R arms carry different gates and are excluded from the
# collapse-class statistics so nothing pools across eras.
ARMS = ("A", "AU", "AL", "ALU")

# The uniflip battery's own collapse threshold, verbatim from goal-gate
# "Store-Cap Unification -- RESULTS" ("Pooled collapse-mode rate (< 60 Mbit/s)").
# NOT chosen here.
COLLAPSE_MBPS = 60.0


def load(d):
    out = []
    for f in sorted(glob.glob(os.path.join(d, "*.log")) + glob.glob(os.path.join(d, "*.out"))):
        for line in open(f, errors="replace"):
            m = RE.search(line)
            if not m:
                continue
            try:
                r = json.loads(m.group(1))
            except Exception:
                continue
            if r.get("wait_tun") is None or r.get("mbps") is None:
                continue
            r["_log"] = os.path.basename(f)
            out.append(r)
    return out


def med(rows, f):
    xs = [r[f] for r in rows if r.get(f) is not None]
    return st.median(xs) if xs else float("nan")


def main():
    d = sys.argv[1] if len(sys.argv) > 1 else os.path.join(
        os.path.dirname(__file__), "..", "..", "docs", "l1-raw")
    recs = load(d)
    print(f"# {len(recs)} per-rep records carrying the wait attribution, from {d}\n")

    print("## THE WAIT ATTRIBUTION PER CELL AND ARM (median over reps of the "
          "per-rep median over DIAG windows)\n")
    print(f"{'cell':8}{'arm':6}{'n':>4} {'tun%':>6}{'paused%':>8}{'nack%':>7}"
          f"{'tail%':>7} {'mbps':>8}{'occ':>7}{'cap':>7}{'q_p50':>7}")
    keys = sorted({(r.get("cell"), r.get("arm")) for r in recs}, key=lambda k: (str(k[0]), str(k[1])))
    for k in keys:
        v = [r for r in recs if (r.get("cell"), r.get("arm")) == k]
        print(f"{str(k[0]):8}{str(k[1]):6}{len(v):>4} {med(v,'wait_tun'):>6.0f}"
              f"{med(v,'wait_paused'):>8.0f}{med(v,'wait_nack'):>7.0f}{med(v,'wait_tail'):>7.0f}"
              f" {med(v,'mbps'):>8.1f}{med(v,'occ_p50'):>7.0f}{med(v,'occcap_p50'):>7.0f}"
              f"{med(v,'q_p50'):>7.0f}")

    print(f"\n## THE c8 COLLAPSE CLASS (< {COLLAPSE_MBPS:g} Mbit/s), arms {ARMS}\n")
    C = [r for r in recs if r.get("cell") == "c8" and r.get("arm") in ARMS]
    col = [r for r in C if r["mbps"] < COLLAPSE_MBPS]
    ok = [r for r in C if r["mbps"] >= COLLAPSE_MBPS]
    print(f"collapse n={len(col)}   normal n={len(ok)}\n")
    print(f"{'field':13}{'collapse':>10}{'normal':>10}{'ratio':>8}")
    for f in ("mbps", "seconds", "wait_tun", "wait_paused", "wait_nack", "wait_tail",
              "occ_p50", "q_p50", "retx", "tc_drop", "tc_pkts", "sf_ticks", "wait_lines"):
        a, b = med(col, f), med(ok, f)
        rr = a / b if b else float("nan")
        print(f"{f:13}{a:>10.1f}{b:>10.1f}{rr:>8.3f}")

    n = sum(1 for r in col if r["wait_tun"] == 0 and r["wait_paused"] == 0)
    m = sum(1 for r in ok if r["wait_tun"] == 0 and r["wait_paused"] == 0)
    print(f"\n`wait_tun` == 0 AND `wait_paused` == 0 :  collapse {n}/{len(col)}   normal {m}/{len(ok)}")

    allc = sorted(C, key=lambda r: r["mbps"])
    z = [i for i, r in enumerate(allc) if r["wait_tun"] == 0 and r["wait_paused"] == 0]
    pref = 0
    while pref < len(allc) and allc[pref]["wait_tun"] == 0 and allc[pref]["wait_paused"] == 0:
        pref += 1
    print(f"ranks of that class among {len(allc)} reps sorted SLOWEST first: {z}")
    print(f"THE {pref} SLOWEST REPS AT c8 ARE ALL IN IT (an unbroken prefix).")

    print("\n## THE APPENDED-TAIL ARITHMETIC -- the wire volume and the emission "
          "work are UNCHANGED; only the wall grows\n")
    cs = [r for r in col if r.get("sf_ticks")]
    os_ = [r for r in ok if r.get("sf_ticks")]
    duty = st.median([r["sf_ticks"] / r["seconds"] for r in os_])
    print(f"normal-class dyn-cap refresh duty:  {duty:.1f} ticks/s "
          f"({100*duty*0.005:.0f}% of the 5 ms throttle)")
    for lbl, g in (("collapse", cs), ("normal", os_)):
        em = st.median([r["sf_ticks"] / duty for r in g])
        wall = st.median([r["seconds"] for r in g])
        print(f"  {lbl:9} wall {wall:5.2f}s   implied emission phase {em:5.2f}s"
              f"   RESIDUAL non-emission wall {wall-em:5.2f}s  ({100*(wall-em)/wall:4.1f}%)")


if __name__ == "__main__":
    main()
