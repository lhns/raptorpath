#!/usr/bin/env python3
"""Markdown tables for docs/derivations/guard-clause-dry.md from res-*.json (score.py), d80-*.json / d80s-*.json (resolution.py) and rowvals.
usage: python -I make_tables.py            (run in this directory; prints the tables)"""
import json
import os

HERE = os.path.dirname(os.path.abspath(__file__))
LEDGERS = [  # (title, res file, d80 stem, rowvals-backed none counts via res)
    ("s52 guard (BASE 12 vs CAND 12; the §52 merge-candidate guard, the ledger of §53 Erratum 1)", "res-s52-guard", "s52-guard"),
    ("bbr48b (BASE 12 vs CAND 12; the §48b bbr_rs battery, a different candidate on the same four cells)", "res-bbr48b", "bbr48b"),
    ("s53 c8 600 MB (BASE vs CAND, then BASES vs CANDS; n = 6 per arm)", "res-s53-c8", "s53-c8"),
    ("s53 c1d 5 GB (BASE 12 vs CAND 12)", "res-s53-c1d", None),
    ("rep52 c8 600 MB (BASE vs CAND; n = 6 per arm)", "res-rep52", "rep52"),
    ("place41b (BASE 12 vs MAIN 6, gate off; c1s and c1d)", "res-p41b", "p41b"),
]
NAMES = {"C": "C rtp_floor", "A": "A src-phase floor", "Bmean": "B mean delay", "Bp95": "B p95 delay", "Bsmean": "B' mean (src phase)", "Bsp95": "B' p95 (src phase)"}
ORDER = ["C", "A", "Bmean", "Bp95", "Bsmean", "Bsp95"]


def j(path):
    p = os.path.join(HERE, path)
    return json.load(open(p)) if os.path.exists(p) else {}


def fm(x):
    if x is None:
        return "-"
    return "%.0f" % x if abs(x) >= 100 else "%.1f" % x


def arm(a):
    return "%s [%s-%s] %d%%" % (fm(a["med"]), fm(a["min"]), fm(a["max"]), round(100 * a["spread"])) + (" (%d None)" % a["n_none"] if a["n_none"] else "")


for title, res, stem in LEDGERS:
    r = j(res + ".json")
    d = {}
    if stem:
        d.update(j("d80-" + stem + ".json"))
        d.update(j("d80s-" + stem + ".json"))
    print("\n#### %s\n" % title)
    print("| cell | leg | clause (unit) | ref arm: median [min-max] spread | test arm: median [min-max] spread | test/ref | p (1-sided) | A/A d80 |\n|---|---|---|---|---|---|---|---|")
    pairs = []
    for k in r:
        pr = k.split("|")[3]
        if pr not in pairs:
            pairs.append(pr)
    for pi, pr in enumerate(pairs):
        ref, test = pr.split(">")
        if len(pairs) > 1:
            print("| **%s vs %s** | | | | | | | |" % (ref, test))
        cells = []
        for k in r:
            if k.split("|")[3] == pr and k.split("|")[0] not in cells:
                cells.append(k.split("|")[0])
        for cell in cells:
            for leg in ("p0", "p1"):
                for cand in ORDER:
                    k = "|".join([cell, cand, leg, pr])
                    if k not in r:
                        continue
                    e = r[k]
                    dv = d.get("|".join([cell, cand, leg]), {}).get("d80", "n/a") if pi == 0 else "n/a"
                    dtxt = "-" if dv == "n/a" else (">100 %" if dv is None else "%g %%" % (100 * dv))
                    print("| %s | %s | %s (%s) | %s | %s | %s | %s | %s |" % (cell, leg, NAMES[cand], e["unit"], arm(e["arms"][ref]), arm(e["arms"][test]),
                                                                          "-" if e["ratio"] is None else "%.3f" % e["ratio"], "-" if e["p"] is None else "%.4f" % e["p"], dtxt))
    extra = {}
    for k, e in r.items():
        for a, v in e["arms"].items():
            if a not in k.split("|")[3].split(">"):
                extra.setdefault((k.split("|")[0], k.split("|")[1], k.split("|")[2]), {})[a] = v
    if extra:
        print("\nOther arms in the same ledger (median [min-max] spread):\n")
        print("| cell | leg | clause | arm | value |\n|---|---|---|---|---|")
        seen = set()
        for (cell, cand, leg), arms in extra.items():
            for a, v in arms.items():
                if (cell, cand, leg, a) not in seen:
                    seen.add((cell, cand, leg, a))
                    print("| %s | %s | %s | %s | %s |" % (cell, leg, NAMES[cand], a, arm(v)))
