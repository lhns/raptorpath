#!/usr/bin/env python3
"""Compare surviving mutants against tools/ci/mutants-baseline.txt.

usage: mutants_compare.py [--outcomes EXPECTED_TOTAL OUTCOMES.json,...] BASELINE MISSED.txt [...]
       mutants_compare.py --write BASELINE MISSED.txt [...]

--outcomes first checks that the shards together ran every listed mutant
(a cut-off shard would otherwise look like fewer survivors).

Lines are normalised (":line:col" removed) so edits that only shift code do
not churn the baseline. Entries are compared as a multiset (the same
description can occur twice in one file).
Exit 1 on a NEW survivor (not in the baseline) or a STALE baseline entry
(now caught or removed); both need a human decision.
"""
import re, sys
from collections import Counter

def norm(line):
    return re.sub(r"^([^:\s]+):\d+:\d+:", r"\1:", line.strip())

def load(paths):
    c = Counter()
    for p in paths:
        with open(p, encoding="utf-8") as f:
            for l in f:
                if l.strip() and not l.startswith("#"):
                    c[norm(l)] += 1
    return c

def check_complete(expected, outcome_files):
    import json
    tot = {"total_mutants": 0, "caught": 0, "missed": 0, "timeout": 0, "unviable": 0}
    for p in outcome_files:
        o = json.load(open(p, encoding="utf-8"))
        for k in tot:
            tot[k] += o.get(k, 0)
    done = tot["caught"] + tot["missed"] + tot["timeout"] + tot["unviable"]
    print("outcomes:", tot, "expected", expected)
    if tot["total_mutants"] != expected or done != expected:
        print("INCOMPLETE: shards did not run every listed mutant")
        return False
    return True

def main(argv):
    if argv[:1] == ["--outcomes"]:
        if not check_complete(int(argv[1]), argv[2].split(",")):
            return 1
        argv = argv[3:]
    write = argv[:1] == ["--write"]
    if write:
        argv = argv[1:]
    base_path, found = argv[0], load(argv[1:])
    if write:
        with open(base_path, "w", encoding="utf-8", newline="\n") as f:
            f.write("# surviving mutants (cargo-mutants missed.txt, line:col stripped); see tools/ci/mutants_compare.py\n")
            for k in sorted(found.elements()):
                f.write(k + "\n")
        print(f"wrote {sum(found.values())} entries to {base_path}")
        return 0
    base = load([base_path])
    new = found - base
    stale = base - found
    for k, n in sorted(new.items()):
        print(f"NEW SURVIVOR x{n}: {k}")
    for k, n in sorted(stale.items()):
        print(f"STALE BASELINE (now caught or gone) x{n}: {k}")
    print(f"survivors {sum(found.values())}, baseline {sum(base.values())}, new {sum(new.values())}, stale {sum(stale.values())}")
    return 1 if new or stale else 0

sys.exit(main(sys.argv[1:]))
