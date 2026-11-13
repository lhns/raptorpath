#!/usr/bin/env python3
"""§48b follow-up facts: (1) row -> arm -> binary / env mapping from the ledger, (2) B mean and B' mean per row at c2 and c8, (3) mean row wall time
per cell in the §52 guard and §48b ledgers.   usage: python -I facts48b.py   (from this directory; reads docs/l1-raw and rowvals-*.json)"""
import collections
import json
import os
import re
import statistics as st
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
R = os.path.join(HERE, "..", "..", "l1-raw")
sys.path.insert(0, os.path.join(HERE, "..", "..", "..", "tools", "l1"))
import place38_parse as pp  # noqa: E402

LD48 = os.path.join(R, "bbr48b", "run", "bbr48", "ld.log")
LD52 = os.path.join(R, "s52", "guard-ld.log")

print("## arm -> binary / env, from the ledger's `=== rep=` header lines and the P38ROW fields\n")
hdr = collections.Counter()
for ln in open(LD48, errors="replace"):
    m = re.match(r"=== rep=\d+ seed=\d+ cell=(\S+) arm=(\S+) .*?arm_env='([^']*)' bin=(\S+) sha=(\w+)", ln)
    if m:
        hdr[(m.group(2), m.group(3) or "(none; RWM_QUIC_CC is `env -u` cleared)", m.group(4), m.group(5))] += 1
rows, _ = pp.rows_of([LD48])
print("| arm | header lines | arm_env | binary | sha (header) | P38ROW bin_sha | rows |\n|---|---|---|---|---|---|---|")
shas = collections.defaultdict(set)
cnt = collections.Counter()
for r in rows:
    shas[r["arm"]].add(r["bin_sha"][:16])
    cnt[r["arm"]] += 1
for (arm, env, b, sha), n in sorted(hdr.items()):
    print("| %s | %d | %s | %s | %s | %s | %d |" % (arm, n, env, b, sha, ",".join(shas[arm]), cnt[arm]))

rv = json.load(open(os.path.join(HERE, "rowvals-bbr48b.json")))
print("\n## B mean and B' mean (ms) per row, seed s42 reps 1-6 then s7 reps 1-6\n")
print("| cell | arm | clause | median | rows (s42 r1..r6 / s7 r1..r6) |\n|---|---|---|---|---|")
for cell in ("c2-100", "c8-100"):
    for arm in ("BASE", "CAND"):
        for lab, key in (("B mean", "B"), ("B' mean (src>0)", "Bs")):
            ks = sorted((k for k in rv if k.startswith("%s|%s|" % (cell, arm))), key=lambda k: (int(k.split("|")[2]), int(k.split("|")[3])))
            vals = [rv[k][key]["mean"] * 1e3 for k in ks]
            s42 = ["%.0f" % v for k, v in zip(ks, vals) if k.split("|")[2] == "42"]
            s7 = ["%.0f" % v for k, v in zip(ks, vals) if k.split("|")[2] == "7"]
            print("| %s | %s | %s | %.0f | %s / %s |" % (cell, arm, lab, st.median(vals), " ".join(s42), " ".join(s7)))

print("\n## mean row wall time (s) per cell, from P38ROW wall_s\n")
for name, ld in (("s52 guard", LD52), ("48b", LD48)):
    rr, _ = pp.rows_of([ld])
    rr, _ = pp.apply_replacements(rr)
    by = collections.defaultdict(list)
    for r in rr:
        by[r["cell"]].append(r["wall_s"])
    tot = sum(sum(v) for v in by.values())
    print(name, {c: (len(v), round(st.mean(v), 1)) for c, v in sorted(by.items())}, "total wall of the rows %.0f s = %.1f min" % (tot, tot / 60))
