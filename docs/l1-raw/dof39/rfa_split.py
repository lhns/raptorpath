"""Status §39: the r > 0 battery's [RFA] classes per source symbol.

Usage:
  git show db30e43:raptorpath/docs/l1-raw/rbattery-trunc/r-s42.log > r-s42.log
  python3 -I rfa_split.py r-s42.log

Reads the RRESULT rows (reps 1-4, the scored truncation) and prints, per
cell / size / arm, the mean of each class over cum_src, plus the per-rep
values of preempt_src (the class the DoF ledger can remove, paper §4.13).
"""
import collections
import json
import sys

CLASSES = ("rfa_fill_coded", "rfa_fill_src", "rfa_dup_src", "rfa_preempt_src", "rfa_rep_redundant")


def main(path):
    rows = [json.loads(l.split("RRESULT ", 1)[1])
            for l in open(path, encoding="utf-8", errors="replace")
            if l.startswith("RRESULT ")]
    groups = collections.defaultdict(list)
    for r in rows:
        # A row whose [DIAG] cum= was cut (sc2/25 MB CTL rep 2) has no
        # denominator and is skipped, as the battery scored it.
        if r["rep"] <= 4 and r.get("cum_src"):
            groups[(r["cell"], r["size"], r["arm"])].append(r)
    print("cell size arm n " + " ".join(c[4:] for c in CLASSES)
          + " nonnack_coded completion_p50 | preempt per rep")
    for key in sorted(groups):
        g = groups[key]
        frac = {c: sum(r[c] / r["cum_src"] for r in g) / len(g) for c in CLASSES}
        coded = sum((r["cum_cod"] - r["diag_retx"]) / r["cum_src"] for r in g) / len(g)
        p50 = sum(r["completion_p50"] for r in g) / len(g)
        per_rep = [round(100 * r["rfa_preempt_src"] / r["cum_src"], 2) for r in g]
        print(*key, len(g), *("%.2f%%" % (100 * frac[c]) for c in CLASSES),
              "%.2f%%" % (100 * coded), "%.2fs" % p50, "|", per_rep)


if __name__ == "__main__":
    main(sys.argv[1])
